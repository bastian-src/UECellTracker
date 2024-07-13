use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use casual_logger::{Level, Log};
use logger::{deploy_logger, LoggerArgs, LoggerState};
use std::collections::HashSet;
use std::error::Error;
use std::fs;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::Duration;

mod cell_info;
mod logger;
mod logic;
mod math_util;
mod ngscope;
mod parse;
mod util;

use logic::cell_source::{deploy_cell_source, CellSourceArgs};
use logic::model_handler::{deploy_model_handler, ModelHandlerArgs};
use logic::ngscope_controller::{deploy_ngscope_controller, NgControlArgs};
use logic::rnti_matcher::{deploy_rnti_matcher, RntiMatcherArgs};
use logic::{
    GeneralState, MainState, MessageCellInfo, MessageDci, MessageRnti, ModelState, NgControlState,
    RntiMatcherState, SourceState, WorkerState, BUS_SIZE_APP_STATE, BUS_SIZE_CELL_INFO,
    BUS_SIZE_DCI, BUS_SIZE_RNTI, CHANNEL_SYNC_SIZE, WORKER_SLEEP_LONG_MS,
};
use logic::{MessageMetric, WorkerChannel, BUS_SIZE_METRIC};
use parse::Arguments;
use util::{determine_process_id, is_notifier, prepare_sigint_notifier, print_info, set_debug};

struct CombinedReceivers {
    pub model: Receiver<ModelState>,
    pub source: Receiver<SourceState>,
    pub rntimatcher: Receiver<RntiMatcherState>,
    pub ngcontrol: Receiver<NgControlState>,
    pub logger: Receiver<LoggerState>,
}

struct CombinedSenders {
    pub model: SyncSender<ModelState>,
    pub source: SyncSender<SourceState>,
    pub rntimatcher: SyncSender<RntiMatcherState>,
    pub ngcontrol: SyncSender<NgControlState>,
    pub logger: SyncSender<LoggerState>,
}

impl CombinedReceivers {
    fn print_worker_messages(&self) {
        let _ = &self.source.worker_print_on_recv();
        let _ = &self.model.worker_print_on_recv();
        let _ = &self.ngcontrol.worker_print_on_recv();
        let _ = &self.rntimatcher.worker_print_on_recv();
        let _ = &self.logger.worker_print_on_recv();
    }
}

fn deploy_app(
    tx_app_state: &mut Bus<MainState>,
    app_args: &Arguments,
    all_tx_states: CombinedSenders,
) -> Result<Vec<JoinHandle<()>>> {
    let mut tx_dci: Bus<MessageDci> = Bus::<MessageDci>::new(BUS_SIZE_DCI);
    let mut tx_cell_info: Bus<MessageCellInfo> = Bus::<MessageCellInfo>::new(BUS_SIZE_CELL_INFO);
    let mut tx_rnti: Bus<MessageRnti> = Bus::<MessageRnti>::new(BUS_SIZE_RNTI);
    let mut tx_metric: Bus<MessageMetric> = Bus::<MessageMetric>::new(BUS_SIZE_METRIC);
    let rx_metric: BusReader<MessageMetric> = tx_metric.add_rx();

    let logger_args = LoggerArgs {
        app_args: app_args.clone(),
        rx_app_state: tx_app_state.add_rx(),
        tx_logger_state: all_tx_states.logger,
    };
    let model_args = ModelHandlerArgs {
        app_args: app_args.clone(),
        rx_app_state: tx_app_state.add_rx(),
        tx_model_state: all_tx_states.model,
        rx_cell_info: tx_cell_info.add_rx(),
        rx_dci: tx_dci.add_rx(),
        rx_rnti: tx_rnti.add_rx(),
        tx_metric,
    };
    let rntimatcher_args = RntiMatcherArgs {
        app_args: app_args.clone(),
        rx_app_state: tx_app_state.add_rx(),
        tx_rntimatcher_state: all_tx_states.rntimatcher,
        rx_dci: tx_dci.add_rx(),
        tx_rnti,
        rx_metric,
    };
    let ngcontrol_args = NgControlArgs {
        app_args: app_args.clone(),
        rx_app_state: tx_app_state.add_rx(),
        tx_ngcontrol_state: all_tx_states.ngcontrol,
        rx_cell_info: tx_cell_info.add_rx(),
        tx_dci,
    };
    let source_args = CellSourceArgs {
        app_args: app_args.clone(),
        rx_app_state: tx_app_state.add_rx(),
        tx_source_state: all_tx_states.source,
        tx_cell_info,
    };

    let tasks: Vec<JoinHandle<()>> = vec![
        deploy_ngscope_controller(ngcontrol_args)?,
        deploy_cell_source(source_args)?,
        deploy_model_handler(model_args)?,
        deploy_rnti_matcher(rntimatcher_args)?,
        deploy_logger(logger_args)?,
    ];
    Ok(tasks)
}

fn check_running<T: WorkerState>(rx_state: &Receiver<T>) -> Result<Option<()>> {
    if let Ok(Some(msg)) = rx_state.worker_try_recv_general_state() {
        match msg {
            GeneralState::Running => {
                print_info(&format!(" ✓ {:?} running", T::worker_name()));
                return Ok(Some(()));
            }
            GeneralState::Stopped => {
                print_info(&format!(" ✗ {:?} stopped", T::worker_name()));
                return Err(anyhow!(
                    "Waiting for all workers to be running, but {:?} sent GeneralState::Stopped",
                    T::worker_name(),
                ));
            }
            GeneralState::Unknown => {
                return Err(anyhow!(
                    "Waiting for all workers to be running, but {:?} sent: {:?}",
                    T::worker_name(),
                    msg,
                ))
            }
        }
    }
    Ok(None)
}

fn wait_all_running(
    sigint_notifier: &Arc<AtomicBool>,
    all_rx_states: &CombinedReceivers,
) -> Result<()> {
    print_info("[ ] waiting for all threads to become ready");

    let mut waiting_for: HashSet<&str> =
        vec!["source", "model", "rntimatcher", "ngcontrol", "logger"]
            .into_iter()
            .collect();

    while !waiting_for.is_empty() {
        if is_notifier(sigint_notifier) {
            return Err(anyhow!(
                "SIGINT while waiting for all workers to be running"
            ));
        }
        if waiting_for.contains("source") {
            if let Ok(Some(_)) = check_running(&all_rx_states.source) {
                waiting_for.remove("source");
            }
        }
        if waiting_for.contains("model") {
            if let Ok(Some(_)) = check_running(&all_rx_states.model) {
                waiting_for.remove("model");
            }
        }
        if waiting_for.contains("rntimatcher") {
            if let Ok(Some(_)) = check_running(&all_rx_states.rntimatcher) {
                waiting_for.remove("rntimatcher");
            }
        }
        if waiting_for.contains("ngcontrol") {
            if let Ok(Some(_)) = check_running(&all_rx_states.ngcontrol) {
                waiting_for.remove("ngcontrol");
            }
        }
        if waiting_for.contains("logger") {
            if let Ok(Some(_)) = check_running(&all_rx_states.logger) {
                waiting_for.remove("logger");
            }
        }
    }

    print_info("[✓] waiting for all threads to become ready");
    Ok(())
}

fn init_logger() -> Result<()> {
    fs::create_dir_all("./.logs")?;
    Log::set_file_name("./.logs/log");
    Log::set_level(Level::Debug);
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    init_logger()?;
    print_info("Hello, world!");
    let args: Arguments = Arguments::build()?;
    set_debug(args.verbose.unwrap());

    let sigint_notifier = prepare_sigint_notifier()?;

    let mut tx_app_state = Bus::<MainState>::new(BUS_SIZE_APP_STATE);
    let (model_tx, model_rx) = sync_channel::<ModelState>(CHANNEL_SYNC_SIZE);
    let (source_tx, source_rx) = sync_channel::<SourceState>(CHANNEL_SYNC_SIZE);
    let (rntimatcher_tx, rntimatcher_rx) = sync_channel::<RntiMatcherState>(CHANNEL_SYNC_SIZE);
    let (ngcontrol_tx, ngcontrol_rx) = sync_channel::<NgControlState>(CHANNEL_SYNC_SIZE);
    let (logger_tx, logger_rx) = sync_channel::<LoggerState>(CHANNEL_SYNC_SIZE);
    let all_tx_states = CombinedSenders {
        model: model_tx,
        source: source_tx,
        rntimatcher: rntimatcher_tx,
        ngcontrol: ngcontrol_tx,
        logger: logger_tx,
    };
    let all_rx_states = CombinedReceivers {
        model: model_rx,
        source: source_rx,
        rntimatcher: rntimatcher_rx,
        ngcontrol: ngcontrol_rx,
        logger: logger_rx,
    };

    let tasks = deploy_app(&mut tx_app_state, &args, all_tx_states)?;

    wait_all_running(&sigint_notifier, &all_rx_states)?;
    print_info(&format!("[main]: \t\tPID {:?}", determine_process_id()));

    let mut app_state: MainState = MainState::Running;
    tx_app_state.broadcast(app_state);

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(WORKER_SLEEP_LONG_MS));
        if is_notifier(&sigint_notifier) && app_state != MainState::Stopped {
            app_state = MainState::NotifyStop;
        }
        /* </precheck> */

        match app_state {
            MainState::Running => app_state = handle_running(&mut tx_app_state, &all_rx_states)?,
            MainState::Stopped => {
                all_rx_states.print_worker_messages();
                if tasks.iter().all(|task| task.is_finished()) {
                    break;
                }
            }
            MainState::NotifyStop => {
                app_state = MainState::Stopped;
                tx_app_state.broadcast(app_state);
            }
            _ => {}
        }
    }

    Ok(())
}

fn handle_running(
    tx_app_state: &mut Bus<MainState>,
    rx_states: &CombinedReceivers,
) -> Result<MainState> {
    if let Some(NgControlState::SuccessfulTriggerResponse) =
        rx_states.ngcontrol.worker_try_recv()?
    {
        tx_app_state.broadcast(MainState::UeConnectionReset);
    }

    Ok(MainState::Running)
}
