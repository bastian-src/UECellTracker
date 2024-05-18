use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::fs::{File, self};
use std::net::UdpSocket;
use std::path::Path;
use std::process::{Child, Stdio};
use std::sync::mpsc::{SyncSender, TryRecvError, Receiver, sync_channel};
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cell_info::CellInfo;
use crate::logic::{
    check_not_stopped, wait_until_running, send_explicit_state,
    MessageCellInfo, MessageDci, WorkerState, WorkerType,
    ExplicitWorkerState, NgControlState,
    DEFAULT_WORKER_SLEEP_MS, CHANNEL_SYNC_SIZE
};
use crate::ngscope;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::Message;
use crate::ngscope::{
    start_ngscope, stop_ngscope, ngscope_validate_server_send_initial, ngscope_validate_server_check
};
use crate::parse::{Arguments, FlattenedNgScopeArgs};


#[derive(Clone, Copy, Debug, PartialEq)]
enum LocalDciState {
    Stop,
    SendInitial,
    WaitForServerAuth(u8),
    ListenForDci,
}

pub struct NgControlArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_ngcontrol_state: SyncSender<WorkerState>,
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

fn send_final_state(tx_sink_state: &SyncSender<WorkerState>) -> Result<()> {
    Ok(tx_sink_state.send(WorkerState::Stopped(WorkerType::CellSource))?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<WorkerState>,
    tx_source_state: &SyncSender<WorkerState>,
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
    tx_ngcontrol_state: SyncSender<WorkerState>,
    app_args: Arguments,
    mut rx_cell_info: BusReader<MessageCellInfo>,
    tx_dci: Bus<MessageDci>,
) -> Result<()> {
    tx_ngcontrol_state.send(WorkerState::Running(WorkerType::NgScopeController))?;
    wait_for_running(&mut rx_app_state, &tx_ngcontrol_state)?;

    let ng_args = FlattenedNgScopeArgs::from_unflattened(app_args.ngscope.unwrap())?;
    let mut ng_process_option: Option<Child> = None;
    let ngscope_config = NgScopeConfig {
        ..Default::default()
    };

    let (tx_dci_thread, rx_dci_thread) = sync_channel::<LocalDciState>(CHANNEL_SYNC_SIZE);
    let dci_thread = deploy_dci_fetcher_thread(
        rx_dci_thread,
        tx_dci,
        ng_args.ng_local_addr.to_string(),
        ng_args.ng_server_addr.to_string(),
    )?;

    let mut ngcontrol_state: NgControlState = NgControlState::CheckingCellInfo;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if check_not_stopped(&mut rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        match ngcontrol_state {
            NgControlState::CheckingCellInfo => {
                ngcontrol_state = handle_cell_update(&mut rx_cell_info,
                                                     &ngscope_config)?;
            },
            NgControlState::TriggerListenDci => {
                tx_dci_thread.send(LocalDciState::SendInitial)?;
                ngcontrol_state = NgControlState::CheckingCellInfo;
            },
            NgControlState::SleepMs(time_ms, next_state) => {
                thread::sleep(Duration::from_millis(time_ms));
                ngcontrol_state = *next_state;
            },
            NgControlState::StartNgScope(config) => {
                if let Some(ref mut process) = ng_process_option { stop_ngscope(process)?; }
                (ngcontrol_state, ng_process_option) = handle_start_ngscope(&config, &ng_args)?;
            },
            NgControlState::StopNgScope => {
                if let Some(ref mut process) = ng_process_option { stop_ngscope(process)?; }
                ngcontrol_state = NgControlState::CheckingCellInfo;
            },
            _ => todo!(),
        }
    }

    send_explicit_state(&tx_ngcontrol_state,
                        ExplicitWorkerState::NgControl(
                            NgControlState::StoppingDciFetcherThread)
                        )?;
    tx_dci_thread.send(LocalDciState::Stop)?;
    let _ = dci_thread.join();
    if ng_process_option.is_some() {
        send_explicit_state(&tx_ngcontrol_state,
                            ExplicitWorkerState::NgControl(
                                NgControlState::StoppingNgScopeProcess)
                            )?;
        stop_ngscope(&mut ng_process_option.unwrap())?;
    }
    tx_ngcontrol_state.send(WorkerState::Stopped(WorkerType::NgScopeController))?;
    Ok(())
}

fn handle_start_ngscope(
    ng_conf: &NgScopeConfig,
    ng_args: &FlattenedNgScopeArgs,
) -> Result<(NgControlState, Option<Child>)> {
    let (std_out, std_err) = match &ng_args.ng_log_file {
        Some(path) => {
            if Path::new(path).exists() {
                fs::remove_file(path).unwrap();
            }
            let file_out = File::create(path)?;
            let file_err = file_out.try_clone()?;
            (Stdio::from(file_out), Stdio::from(file_err))
        },
        None => (Stdio::null(), Stdio::null()),
    };
    let new_ng_process = Some(start_ngscope(&ng_args.ng_path, ng_conf, std_out, std_err)?);
    Ok((NgControlState::SleepMs(5000, Box::new(NgControlState::TriggerListenDci)), new_ng_process))
}


fn handle_cell_update(
    rx_cell_info: &mut BusReader<MessageCellInfo>,
    ng_conf: &NgScopeConfig,
) -> Result<NgControlState> {
    match check_cell_update(rx_cell_info)? {
        Some(cell_info) => {
            println!("[ngcontrol] cell_info: {:#?}", cell_info);
            if cell_info.cells.is_empty() {
                return Ok(NgControlState::StopNgScope);
            }
            // TODO: Handle multi cell
            let mut new_conf = ng_conf.clone();
            new_conf.rf_config0.as_mut().unwrap().rf_freq = cell_info.cells.first().unwrap().frequency;
            Ok(NgControlState::StartNgScope(Box::new(new_conf)))
        }
        _ => Ok(NgControlState:: CheckingCellInfo),
    }
}


fn deploy_dci_fetcher_thread(
    rx_local_dci_state: Receiver<LocalDciState>,
    tx_dci: Bus<MessageDci>,
    local_socket_addr: String,
    ng_server_addr: String,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run_dci_fetcher(
            rx_local_dci_state,
            tx_dci,
            local_socket_addr,
            ng_server_addr,
        );
    });
    Ok(thread)
}

fn run_dci_fetcher(
    rx_local_dci_state: Receiver<LocalDciState>,
    mut tx_dci: Bus<MessageDci>,
    local_socket_addr: String,
    ng_server_addr: String,
) -> Result<()> {
    let socket = init_dci_server(&local_socket_addr)?;
    let mut dci_state: LocalDciState = LocalDciState::ListenForDci;

    loop {
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if let Some(new_state) = check_rx_state(&rx_local_dci_state)? {
            dci_state = new_state;
        }

        match dci_state {
            LocalDciState::Stop => { break; },
            LocalDciState::SendInitial => {
                dci_state = match ngscope_validate_server_send_initial(&socket, &ng_server_addr) {
                    Ok(_) => LocalDciState::WaitForServerAuth(0),
                    Err(_) => LocalDciState::SendInitial,
                };
            },
            LocalDciState::WaitForServerAuth(successful_auths) => { 
                dci_state = match ngscope_validate_server_check(&socket)? {
                    Some(_) => {
                        if successful_auths >= 1 {
                            LocalDciState::ListenForDci
                        } else {
                            LocalDciState::WaitForServerAuth(successful_auths + 1)
                        }
                    },
                    None => LocalDciState::WaitForServerAuth(successful_auths),
                };
            },
            LocalDciState::ListenForDci => { check_ngscope_message(&socket, &mut tx_dci) },
        }
    }
    Ok(())
}

fn check_ngscope_message(socket: &UdpSocket, tx_dci: &mut Bus<MessageDci>) {
    match ngscope::ngscope_recv_single_message(socket) {
        Ok(msg) => {
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

                    // TODO: Take items from the bus.
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
        },
        _ => {
            // TODO: print error properly? it also goes here when there just hasn't been a message
        }
    }
}

fn check_rx_state(
    rx_local_dci_state: &Receiver<LocalDciState>,
) -> Result<Option<LocalDciState>> {
    match rx_local_dci_state.try_recv() {
        Ok(msg) => Ok(Some(msg)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(anyhow!("[ngcontrol.local_dci_fetcher] rx_local_dci_state disconnected")),
    }
}

#[allow(dead_code)]
fn init_dci_server(local_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr)?;
    socket.set_nonblocking(true)?;
    // ngscope::ngscope_validate_server(&socket, server_addr).expect("server validation error");

    Ok(socket)
}

