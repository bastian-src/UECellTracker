use anyhow::Result;
use bus::{Bus, BusReader};
use std::net::UdpSocket;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::Duration;

use crate::cell_info::CellInfo;
use crate::logic::{check_not_stopped, MessageCellInfo, MessageDci, WorkerState, WorkerType};
use crate::ngscope;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::Message;
use crate::ngscope::{restart_ngscope, start_ngscope, stop_ngscope};
use crate::parse::{Arguments, MilesightArgs};
use crate::util;

pub struct NgControlArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_ngcontrol_state: Sender<WorkerState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub tx_dci: Bus<MessageDci>,
}

pub fn deploy_ngscope_controller(args: NgControlArgs) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(
            args.rx_app_state,
            args.tx_ngcontrol_state,
            args.rx_cell_info,
            args.tx_dci,
        );
    });
    Ok(thread)
}

enum CheckCellUpdateError {
    NoUpdate(BusReader<MessageCellInfo>),
    Disconnected,
}

fn check_cell_update(
    mut rx_cell_info: BusReader<MessageCellInfo>,
    ) -> Result<(CellInfo, BusReader<MessageCellInfo>), CheckCellUpdateError> {
    match rx_cell_info.try_recv() {
        Ok(msg) => {
            Ok((msg.cell_info, rx_cell_info))
        },
        Err(TryRecvError::Empty) => Err(CheckCellUpdateError::NoUpdate(rx_cell_info)),
        Err(TryRecvError::Disconnected) => Err(CheckCellUpdateError::Disconnected),
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_ngcontrol_state: Sender<WorkerState>,
    mut rx_cell_info: BusReader<MessageCellInfo>,
    _tx_dci: Bus<MessageDci>,
) -> Result<()> {
    tx_ngcontrol_state.send(WorkerState::Running(WorkerType::NgScopeController))?;
    thread::sleep(Duration::from_secs(1));

    loop {
        thread::sleep(Duration::from_millis(1));
        match check_not_stopped(rx_app_state) {
            Ok(rx_app) => rx_app_state = rx_app,
            _ => break,
        }
        match check_cell_update(rx_cell_info) {
            Ok((cell_info, returnal)) => {
                println!("[ngcontrol] cell_info: {:#?}", cell_info);
                rx_cell_info = returnal;
            },
            Err(CheckCellUpdateError::NoUpdate(returnal)) => {
                rx_cell_info = returnal;
            },
            Err(CheckCellUpdateError::Disconnected) => {
                break;
            },
        }
        // TODO: Check for new CellInfo -> set ngscope to new frequency
        // TODO: Forward ngsocpe dci messages to tx_dci
        thread::sleep(Duration::from_secs(5));
    }

    tx_ngcontrol_state.send(WorkerState::Stopped(WorkerType::NgScopeController))?;
    Ok(())
}

#[allow(dead_code)]
fn start_continuous_tracking(args: Arguments) -> Result<()> {
    // Retrieve cell information
    // Write config
    // Start ng-scope process
    // loop:
    //   Retrieve cell (did it change?)
    //   Update config
    //   Restart ng-scope process
    //   -> implement hysterese: only restart if it has been running for a while.

    let sigint: Arc<AtomicBool> = util::prepare_sigint_notifier()?;
    let milesight_args: MilesightArgs = args.milesight.unwrap();
    let cell_info: CellInfo = CellInfo::from_milesight_router(
        &milesight_args.clone().milesight_address.unwrap(),
        &milesight_args.clone().milesight_user.unwrap(),
        &milesight_args.clone().milesight_auth.unwrap(),
    )?;
    let mut single_cell = cell_info.cells.first().unwrap().clone();
    let mut ngscope_process: Child;
    let mut ngscope_config = NgScopeConfig {
        rnti: 0xFFFF,
        ..Default::default()
    };

    ngscope_config.rf_config0.as_mut().unwrap().rf_freq = single_cell.frequency;
    ngscope_process = start_ngscope(&ngscope_config)?;

    while !util::is_notifier(&sigint) {
        let latest_cell_info: CellInfo = CellInfo::from_milesight_router(
            &milesight_args.clone().milesight_address.unwrap(),
            &milesight_args.clone().milesight_user.unwrap(),
            &milesight_args.clone().milesight_auth.unwrap(),
        )?;
        let latest_single_cell = latest_cell_info.cells.first().unwrap();
        if latest_single_cell.cell_id != single_cell.cell_id {
            // TODO: Determine the RNIT using RNTI matching
            ngscope_config.rnti = 0xFFFF;
            ngscope_config.rf_config0.as_mut().unwrap().rf_freq = latest_single_cell.frequency;
            ngscope_process = restart_ngscope(ngscope_process, &ngscope_config)?;
            single_cell = latest_single_cell.clone();
        }
        thread::sleep(Duration::from_secs(10));
    }
    stop_ngscope(ngscope_process)?;
    Ok(())
}

#[allow(dead_code)]
fn init_dci_server(local_addr: &str, server_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr).unwrap();
    ngscope::ngscope_validate_server(&socket, server_addr).expect("server validation error");

    Ok(socket)
}

#[allow(dead_code)]
fn start_listen_for_ngscope_message() -> Result<()> {
    let local_addr = "0.0.0.0:8888";
    let server_addr = "0.0.0.0:6767";

    let socket = init_dci_server(local_addr, server_addr)?;

    println!("Successfully initialized Dci server");
    println!("Analyzing incoming messages..");

    loop {
        if let Ok(msg) = ngscope::ngscope_recv_single_message(&socket) {
            match msg {
                Message::Start => {}
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
                }
                Message::Dci(ue_dci) => {
                    println!("{:?}", ue_dci)
                }
                Message::Config(cell_config) => {
                    println!("{:?}", cell_config)
                }
                Message::Exit => {
                    break;
                }
            }
        } else {
            println!("could not receive message..")
        }
    }
    Ok(())
}
