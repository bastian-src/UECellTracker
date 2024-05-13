use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::logic::{WorkerState, WorkerType, MessageCellInfo, check_not_stopped, wait_until_running};

pub struct CellSourceArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_source_state: Sender<WorkerState>,
    pub tx_cell_info: Bus<MessageCellInfo>,
}

pub fn deploy_cell_source(
    args: CellSourceArgs,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(args.rx_app_state,
                    args.tx_source_state,
                    args.tx_cell_info);
    });
    Ok(thread)
}

fn send_final_state(tx_sink_state: &Sender<WorkerState>) -> Result<()> {
    Ok(tx_sink_state.send(WorkerState::Stopped(WorkerType::CellSource))?)
}

fn wait_for_running(
        rx_app_state: BusReader<WorkerState>,
        tx_source_state: &Sender<WorkerState>,
) -> Result<BusReader<WorkerState>> {
    match wait_until_running(rx_app_state) {
        Ok(rx_app) => Ok(rx_app),
        _ => {
            send_final_state(tx_source_state)?;
            Err(anyhow!("[source] Main did not send 'Running' message"))
        }
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_source_state: Sender<WorkerState>,
    _tx_cell_info: Bus<MessageCellInfo>,
) -> Result<()> {
    tx_source_state.send(WorkerState::Running(WorkerType::CellSource))?;
    rx_app_state = wait_for_running(rx_app_state, &tx_source_state)?;

    loop {
        thread::sleep(Duration::from_millis(1));
        match check_not_stopped(rx_app_state) {
            Ok(rx_app) => rx_app_state = rx_app,
            _ => break,
        }

        // TODO: Retrieve CellInfo
        // TODO: If changed, send CellInfo to tx_cell_info
        thread::sleep(Duration::from_secs(5));
    }

    send_final_state(&tx_source_state)?;
    Ok(())
}
