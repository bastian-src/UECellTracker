use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::net::UdpSocket;
use std::process::Child;
use std::sync::mpsc::{Sender, TryRecvError, Receiver, channel};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cell_info::CellInfo;
use crate::logic::{
    check_not_stopped, wait_until_running, send_explicit_state,
    MessageCellInfo, MessageDci, WorkerState, WorkerType,
    ExplicitWorkerState, NgControlState,
    DEFAULT_WORKER_SLEEP_MS,
};
use crate::ngscope;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::Message;
use crate::ngscope::{restart_ngscope, start_ngscope, stop_ngscope};
use crate::parse::Arguments;


enum LocalDciState {
    Stop,
}

pub struct NgControlArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_ngcontrol_state: Sender<WorkerState>,
    pub app_args: Arguments,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub tx_dci: Bus<MessageDci>,
}

pub fn deploy_ngscope_controller(args: NgControlArgs) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(
            args.rx_app_state,
            args.tx_ngcontrol_state,
            args.app_args,
            args.rx_cell_info,
            args.tx_dci,
        );
    });
    Ok(thread)
}

fn send_final_state(tx_sink_state: &Sender<WorkerState>) -> Result<()> {
    Ok(tx_sink_state.send(WorkerState::Stopped(WorkerType::CellSource))?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<WorkerState>,
    tx_source_state: &Sender<WorkerState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_source_state)?;
            Err(anyhow!("[source] Main did not send 'Running' message"))
        }
    }
}

fn check_cell_update(
    rx_cell_info: &mut BusReader<MessageCellInfo>,
) -> Result<Option<CellInfo>> {
    match rx_cell_info.try_recv() {
        Ok(msg) => {
            Ok(Some(msg.cell_info))
        },
        Err(TryRecvError::Disconnected) => {
            // TODO: print error properly
            Err(anyhow!("[ngcontrol] err: rx_cell_info disconnected!"))
        }
        Err(TryRecvError::Empty) => { Ok(None) },
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_ngcontrol_state: Sender<WorkerState>,
    app_args: Arguments,
    mut rx_cell_info: BusReader<MessageCellInfo>,
    tx_dci: Bus<MessageDci>,
) -> Result<()> {
    tx_ngcontrol_state.send(WorkerState::Running(WorkerType::NgScopeController))?;
    wait_for_running(&mut rx_app_state, &tx_ngcontrol_state)?;

    let mut ng_process_option: Option<Child> = None;
    let mut ngscope_config = NgScopeConfig {
        ..Default::default()
    };

    let (tx_dci_thread, rx_dci_thread) = channel::<LocalDciState>();
    let dci_thread = deploy_dci_fetcher_thread(
        rx_dci_thread,
        tx_dci,
    )?;

    let mut ngcontrol_state: NgControlState = NgControlState::CheckingInitialCellInfo;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if check_not_stopped(&mut rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        match ngcontrol_state {
            NgControlState::CheckingInitialCellInfo => {
                if let Some(cell_info) = check_cell_update(&mut rx_cell_info)? {
                    println!("[ngcontrol] cell_info: {:#?}", cell_info);
                    ngscope_config.rf_config0.as_mut().unwrap().rf_freq = cell_info.cells.first().unwrap().frequency;
                    ng_process_option = Some(start_ngscope(&ngscope_config)?);
                    ngcontrol_state = NgControlState::CheckingCellInfo;
                }
            },
            NgControlState::CheckingCellInfo => {
                if let Some(cell_info) = check_cell_update(&mut rx_cell_info)? {
                    println!("[ngcontrol] cell_info: {:#?}", cell_info);
                    ngscope_config.rf_config0.as_mut().unwrap().rf_freq = cell_info.cells.first().unwrap().frequency;
                    ng_process_option = match ng_process_option {
                        Some(process) => { Some(restart_ngscope(process, &ngscope_config)?) },
                        None => { Some(start_ngscope(&ngscope_config)?) },
                    };
                    ngcontrol_state = NgControlState::CheckingCellInfo;
                }
            },
            _ => {},
        }
    }

    send_explicit_state(&tx_ngcontrol_state,
                        ExplicitWorkerState::NgControl(
                            NgControlState::StoppingDciFetcherThread)
                        )?;
    tx_dci_thread.send(LocalDciState::Stop)?;
    let _ = dci_thread.join();
    if let Some(process) = ng_process_option {
        send_explicit_state(&tx_ngcontrol_state,
                            ExplicitWorkerState::NgControl(
                                NgControlState::StoppingNgScopeProcess)
                            )?;
        stop_ngscope(process)?;
    }
    tx_ngcontrol_state.send(WorkerState::Stopped(WorkerType::NgScopeController))?;
    Ok(())
}

fn deploy_dci_fetcher_thread(
    rx_local_dci_state: Receiver<LocalDciState>,
    tx_dci: Bus<MessageDci>,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run_dci_fetcher(
            rx_local_dci_state,
            tx_dci
        );
    });
    Ok(thread)
}

fn run_dci_fetcher(
    rx_local_dci_state: Receiver<LocalDciState>,
    mut tx_dci: Bus<MessageDci>,
) -> Result<()> {
    // TODO: pass ngscope addr:port information
    let local_addr = "0.0.0.0:8888";
    let server_addr = "0.0.0.0:6767";
    let socket = init_dci_server(local_addr, server_addr)?;
    loop {
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        match local_dci_check_not_stopped(&rx_local_dci_state) {
            Ok(_) => {},
            _ => break,
        }

        if let Ok(msg) = ngscope::ngscope_recv_single_message(&socket) {
            match msg {
                Message::CellDci(cell_dci) => {
                    println!(
                        "<THESIS> {:?} | {:03?} | {:03?} | {:08?} | {:08?} | {:03?} | {:03?}",
                        cell_dci.nof_rnti,
                        cell_dci.total_dl_prb,
                        cell_dci.total_ul_prb,
                        cell_dci.total_dl_tbs,
                        cell_dci.total_ul_tbs,
                        cell_dci.total_dl_reTx,
                        cell_dci.total_ul_reTx
                    );

                    tx_dci.broadcast(MessageDci { ngscope_dci: *cell_dci });
                }
                Message::Dci(ue_dci) => {
                    // TODO: Evaluate how to handle this
                    println!("{:?}", ue_dci)
                }
                Message::Config(cell_config) => {
                    // TODO: Evaluate how to handle this
                    println!("{:?}", cell_config)
                }
                // TODO: Evaluate how to handle Start andExit
                Message::Start => {}
                Message::Exit => {}
            }
        } else {
            // TODO: print error properly
            println!("could not receive message..")
        }
    }
    Ok(())
}

fn local_dci_check_not_stopped(
    rx_local_dci_state: &Receiver<LocalDciState>,
) -> Result<()> {
    match rx_local_dci_state.try_recv() {
        Ok(msg) => match msg {
            LocalDciState::Stop => Err(anyhow!("[ngcontrol.local_dci_fetcher] received stop")),
            // _ => Ok(()), <- use this if LocalDciState gets more fields
        },
        Err(TryRecvError::Empty) => Ok(()),
        Err(TryRecvError::Disconnected) => Err(anyhow!("[ngcontrol.local_dci_fetcher] rx_local_dci_state disconnected")),
    }
}

#[allow(dead_code)]
fn init_dci_server(local_addr: &str, server_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr).unwrap();
    ngscope::ngscope_validate_server(&socket, server_addr).expect("server validation error");

    Ok(socket)
}

