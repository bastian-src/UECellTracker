use anyhow::{anyhow, Result};
use bus::{BusReader, Bus};
use std::sync::mpsc::Sender;
use std::thread::{self, JoinHandle};
use std::time::Duration;

use crate::logic::{WorkerState, WorkerType, MessageRnti, MessageDci, check_not_stopped, wait_until_running};

pub struct RntiMatcherArgs {
    pub rx_app_state: BusReader<WorkerState>,
    pub tx_rntimatcher_state: Sender<WorkerState>,
    pub rx_dci: BusReader<MessageDci>,
    pub tx_rnti: Bus<MessageRnti>,
}

pub fn deploy_rnti_matcher(
    args: RntiMatcherArgs,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(args.rx_app_state, args.tx_rntimatcher_state, args.rx_dci, args.tx_rnti);
    });
    Ok(thread)
}

fn send_final_state(tx_rntimatcher_state: &Sender<WorkerState>) -> Result<()> {
    Ok(tx_rntimatcher_state.send(WorkerState::Stopped(WorkerType::RntiMatcher))?)
}

fn wait_for_running(
        rx_app_state: BusReader<WorkerState>,
        tx_rntimtacher_state: &Sender<WorkerState>,
) -> Result<BusReader<WorkerState>> {
    match wait_until_running(rx_app_state) {
        Ok(rx_app) => Ok(rx_app),
        _ => {
            send_final_state(tx_rntimtacher_state)?;
            Err(anyhow!("[sink] Main did not send 'Running' message"))
        }
    }
}

fn run(
    mut rx_app_state: BusReader<WorkerState>,
    tx_rntimatcher_state: Sender<WorkerState>,
    _rx_dci: BusReader<MessageDci>,
    _tx_rnti: Bus<MessageRnti>,
) -> Result<()> {
    tx_rntimatcher_state.send(WorkerState::Running(WorkerType::RntiMatcher))?;
    rx_app_state = wait_for_running(rx_app_state, &tx_rntimatcher_state)?;

    loop {
        thread::sleep(Duration::from_millis(1));
        match check_not_stopped(rx_app_state) {
            Ok(rx_app) => rx_app_state = rx_app,
            _ => break,
        }
        // TODO: Match rntis
        // TODO: Generate upstream pattern? -> maybe in another thread
        thread::sleep(Duration::from_secs(5));
    }

    tx_rntimatcher_state.send(WorkerState::Stopped(WorkerType::RntiMatcher))?;
    Ok(())
}
