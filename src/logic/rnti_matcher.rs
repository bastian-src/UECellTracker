use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use std::sync::mpsc::SyncSender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::logic::{
    check_not_stopped, wait_until_running,
    MessageDci, MessageRnti, WorkerState, WorkerType,
    DEFAULT_WORKER_SLEEP_MS,
};

pub struct RntiMatcherArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_rntimatcher_state: SyncSender<WorkerState>,
    pub rx_dci: BusReader<MessageDci>,
    pub tx_rnti: Bus<MessageRnti>,
}

pub fn deploy_rnti_matcher(args: RntiMatcherArgs) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(
            args.rx_app_state,
            args.tx_rntimatcher_state,
            args.rx_dci,
            args.tx_rnti,
        );
    });
    Ok(thread)
}

fn send_final_state(tx_rntimatcher_state: &SyncSender<WorkerState>) -> Result<()> {
    Ok(tx_rntimatcher_state.send(WorkerState::Stopped(WorkerType::RntiMatcher))?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<WorkerState>,
    tx_rntimtacher_state: &SyncSender<WorkerState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_rntimtacher_state)?;
            Err(anyhow!("[sink] Main did not send 'Running' message"))
        }
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_rntimatcher_state: SyncSender<WorkerState>,
    _rx_dci: BusReader<MessageDci>,
    _tx_rnti: Bus<MessageRnti>,
) -> Result<()> {
    tx_rntimatcher_state.send(WorkerState::Running(WorkerType::RntiMatcher))?;
    wait_for_running(&mut rx_app_state, &tx_rntimatcher_state)?;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if check_not_stopped(&mut rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        // TODO: Match rntis
        // TODO: Generate upstream pattern? -> maybe in another thread
    }

    tx_rntimatcher_state.send(WorkerState::Stopped(WorkerType::RntiMatcher))?;
    Ok(())
}
