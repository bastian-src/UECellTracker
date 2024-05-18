use anyhow::{anyhow, Result};
use bus::Bus;
use std::collections::HashSet;
use std::error::Error;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod cell_info;
mod logic;
mod ngscope;
mod parse;
mod util;

use logic::cell_sink::{deploy_cell_sink, CellSinkArgs};
use logic::cell_source::{deploy_cell_source, CellSourceArgs};
use logic::ngscope_controller::{deploy_ngscope_controller, NgControlArgs};
use logic::rnti_matcher::{deploy_rnti_matcher, RntiMatcherArgs};
use logic::{
    MessageCellInfo, MessageDci, MessageRnti, MainState, WorkerState, WorkerType,
    NUM_OF_WORKERS, DEFAULT_WORKER_SLEEP_MS, BUS_SIZE_APP_STATE, BUS_SIZE_DCI, BUS_SIZE_CELL_INFO, BUS_SIZE_RNTI,
};
use parse::Arguments;
use util::{is_notifier, prepare_sigint_notifier};

fn deploy_app(
    tx_app_state: &mut Bus<WorkerState>,
    app_args: &Arguments,
    tx_sink_state: Sender<WorkerState>,
    tx_source_state: Sender<WorkerState>,
    tx_ngcontrol_state: Sender<WorkerState>,
    tx_rntimatcher_state: Sender<WorkerState>,
) -> Result<Vec<JoinHandle<()>>> {
    let mut tx_dci: Bus<MessageDci> = Bus::<MessageDci>::new(BUS_SIZE_DCI);
    let mut tx_cell_info: Bus<MessageCellInfo> = Bus::<MessageCellInfo>::new(BUS_SIZE_CELL_INFO);
    let mut tx_rnti: Bus<MessageRnti> = Bus::<MessageRnti>::new(BUS_SIZE_RNTI);

    let sink_args = CellSinkArgs {
        rx_app_state: tx_app_state.add_rx(),
        tx_sink_state,
        rx_cell_info: tx_cell_info.add_rx(),
        rx_dci: tx_dci.add_rx(),
        rx_rnti: tx_rnti.add_rx(),
    };
    let rntimatcher_args = RntiMatcherArgs {
        rx_app_state: tx_app_state.add_rx(),
        tx_rntimatcher_state,
        rx_dci: tx_dci.add_rx(),
        tx_rnti,
    };
    let ngcontrol_args = NgControlArgs {
        rx_app_state: tx_app_state.add_rx(),
        tx_ngcontrol_state,
        app_args: app_args.clone(),
        rx_cell_info: tx_cell_info.add_rx(),
        tx_dci,
    };
    let source_args = CellSourceArgs {
        rx_app_state: tx_app_state.add_rx(),
        tx_source_state,
        app_args: app_args.clone(),
        tx_cell_info,
    };

    let tasks: Vec<JoinHandle<()>> = vec![
        deploy_ngscope_controller(ngcontrol_args)?,
        deploy_cell_source(source_args)?,
        deploy_cell_sink(sink_args)?,
        deploy_rnti_matcher(rntimatcher_args)?,
    ];
    Ok(tasks)
}

fn wait_all_running(
    sigint_notifier: &Arc<AtomicBool>,
    rx_states: [&Receiver<WorkerState>; NUM_OF_WORKERS],
) -> Result<()> {
    println!("[ ] waiting for all threads to become ready");

    let mut waiting_for: HashSet<WorkerType> = vec![
        WorkerType::CellSource,
        WorkerType::CellSink,
        WorkerType::NgScopeController,
        WorkerType::RntiMatcher,
    ]
    .into_iter()
    .collect();

    while !waiting_for.is_empty() {
        if is_notifier(sigint_notifier) {
            return Err(anyhow!(
                "SIGINT while waiting for all workers to be running"
            ));
        }
        for rx_state in rx_states.iter() {
            match rx_state.try_recv() {
                Ok(msg) => match msg {
                    WorkerState::Running(worker) => {
                        println!(" ✓ {:?} running", worker);
                        waiting_for.remove(&worker);
                    }
                    WorkerState::Stopped(worker) => {
                        println!(" ✗ {:?} stopped", worker);
                        waiting_for.remove(&worker);
                    }
                    WorkerState::Specific(worker, state) => {
                        return Err(anyhow!(
                            "Waiting for all workers to be running, but {:?} sent: {:?}",
                            worker,
                            state
                        ))
                    }
                },
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    return Err(anyhow!(
                        "Waiting for all workers to be running, \
                        but a channel disconnected. Left workers: {:?}",
                        waiting_for
                    ));
                }
            }
        }
    }

    println!("[✓] waiting for all threads to become ready");
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    println!("Hello, world!");
    let args: Arguments = Arguments::build()?;

    let sigint_notifier = prepare_sigint_notifier()?;

    let mut tx_app_state = Bus::<WorkerState>::new(BUS_SIZE_APP_STATE);
    let (tx_sink_state, rx_sink_state) = channel::<WorkerState>();
    let (tx_source_state, rx_source_state) = channel::<WorkerState>();
    let (tx_ngcontrol_state, rx_ngcontrol_state) = channel::<WorkerState>();
    let (tx_rntimatcher_state, rx_rntimatcher_state) = channel::<WorkerState>();
    let all_rx_states = [
        &rx_source_state,
        &rx_sink_state,
        &rx_ngcontrol_state,
        &rx_rntimatcher_state,
    ];
    let tasks = deploy_app(
        &mut tx_app_state,
        &args,
        tx_sink_state,
        tx_source_state,
        tx_ngcontrol_state,
        tx_rntimatcher_state,
    )?;

    wait_all_running(&sigint_notifier, all_rx_states)?;

    tx_app_state.broadcast(WorkerState::Running(WorkerType::Main));

    let mut app_state: MainState = MainState::Running;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        if is_notifier(&sigint_notifier) && app_state != MainState::Stopping {
            app_state = MainState::NotifyStop;
        }
        /* </precheck> */

        match app_state {
            MainState::Running => print_worker_messages(all_rx_states),
            MainState::Stopping => {
                print_worker_messages(all_rx_states);
                if tasks.iter().all(|task| task.is_finished()) {
                    break;
                }
            },
            MainState::NotifyStop => {
                tx_app_state.broadcast(WorkerState::Stopped(WorkerType::Main));
                app_state = MainState::Stopping;
            },
        }
    }

    Ok(())
}

fn print_worker_messages(
    rx_states: [&Receiver<WorkerState>; NUM_OF_WORKERS],
) {
    for rx_state in rx_states.iter() {
        match rx_state.try_recv() {
            Ok(resp) => {
                let worker_type = match resp {
                    WorkerState::Running(w_type) => w_type,
                    WorkerState::Stopped(w_type) => w_type,
                    WorkerState::Specific(w_type, _) => w_type,
                };
                println!("[main] message from {}: {:#?}", worker_type, resp);
            }
            Err(TryRecvError::Empty) => { /* No message received, continue the loop */ }
            Err(TryRecvError::Disconnected) => {} // Handle disconnection if necessary
        }
    }
}
