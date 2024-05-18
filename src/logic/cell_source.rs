use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::cell_info::CellInfo;
use crate::logic::{
    check_not_stopped, wait_until_running,
    MessageCellInfo, WorkerState, WorkerType,
    DEFAULT_WORKER_SLEEP_MS,
};
use crate::parse::{Arguments, FlattenedCellApiConfig};


const WAIT_TO_RETRIEVE_CELL_INFO_MS: u64 = 500;


pub struct CellSourceArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_source_state: SyncSender<WorkerState>,
    pub app_args: Arguments,
    pub tx_cell_info: Bus<MessageCellInfo>,
}

pub fn deploy_cell_source(args: CellSourceArgs) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(
            args.rx_app_state,
            args.tx_source_state,
            args.app_args,
            args.tx_cell_info,
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

fn retrieve_cell_info(cell_api: &FlattenedCellApiConfig) -> Result<CellInfo> {
    match cell_api {
        FlattenedCellApiConfig::Milesight(m_args) => CellInfo::from_milesight_router(
            &m_args.milesight_address,
            &m_args.milesight_user,
            &m_args.milesight_auth,
        ),
        FlattenedCellApiConfig::DevicePublisher(dp_args) => {
            CellInfo::from_devicepublisher(&dp_args.devpub_address)
        }
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_source_state: SyncSender<WorkerState>,
    app_args: Arguments,
    mut tx_cell_info: Bus<MessageCellInfo>,
) -> Result<()> {
    tx_source_state.send(WorkerState::Running(WorkerType::CellSource))?;
    wait_for_running(&mut rx_app_state, &tx_source_state)?;

    let cell_api_args = FlattenedCellApiConfig::from_unflattened(
        app_args.cellapi.unwrap(),
        app_args.milesight.unwrap(),
        app_args.devicepublisher.unwrap(),
    )?;
    let mut last_cell_info: CellInfo = CellInfo {
        cells: vec![],
    };

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if check_not_stopped(&mut rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        match retrieve_cell_info(&cell_api_args) {
            Ok(cell_info) => {
                if !CellInfo::equal_content(&cell_info, &last_cell_info) {
                    tx_cell_info.broadcast(MessageCellInfo { cell_info: cell_info.clone() });
                    last_cell_info = cell_info;
                }
            },
            Err(some_err) => {
                // TODO: print error properly
                println!("[source] err retriecing cell info: {:#?}", some_err);
            },
        }

        thread::sleep(Duration::from_millis(WAIT_TO_RETRIEVE_CELL_INFO_MS - DEFAULT_WORKER_SLEEP_MS));
    }

    send_final_state(&tx_source_state)?;
    Ok(())
}
