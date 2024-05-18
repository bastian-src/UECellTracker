use std::fmt::Debug;
use std::sync::mpsc::{TryRecvError, SyncSender};
use std::fmt;

use anyhow::Result;
use bus::BusReader;

use crate::cell_info::CellInfo;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::NgScopeCellDci;

pub mod cell_sink;
pub mod cell_source;
pub mod ngscope_controller;
pub mod rnti_matcher;

pub const NUM_OF_WORKERS: usize = 4;
pub const DEFAULT_WORKER_SLEEP_MS: u64 = 1;
pub const CHANNEL_SYNC_SIZE: usize = 10;
pub const BUS_SIZE_APP_STATE: usize = 50;
pub const BUS_SIZE_DCI: usize = 10000;
pub const BUS_SIZE_CELL_INFO: usize = 100;
pub const BUS_SIZE_RNTI: usize = 100;

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
pub enum WorkerType {
    Main,
    CellSource,
    CellSink,
    NgScopeController,
    RntiMatcher,
}

impl fmt::Display for WorkerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let display_str = match self {
            WorkerType::Main => "main",
            WorkerType::CellSource => "cell_source",
            WorkerType::CellSink => "cell_sink",
            WorkerType::NgScopeController => "ngscope_controller",
            WorkerType::RntiMatcher => "rnti_matcher",
        };
        write!(f, "{}", display_str)
    }
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum ExplicitWorkerState {
    Main(MainState),
    Sink(SinkState),
    Source(SourceState),
    RntiMatcher(RntiMatcherState),
    NgControl(NgControlState),
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MainState {
    Running,
    Stopping,
    NotifyStop,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SinkState {
    SpecialState,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SourceState {
    SpecialState,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RntiMatcherState {
    SpecialState,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum NgControlState {
    CheckingCellInfo,
    StartNgScope(Box<NgScopeConfig>),
    StopNgScope,
    TriggerListenDci,
    SleepMs(u64, Box<NgControlState>),
    StoppingDciFetcherThread,
    RestartingNgScopeProcess,
    StoppingNgScopeProcess,
    Idle,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub enum WorkerState {
    Running(WorkerType),
    Stopped(WorkerType),
    Specific(WorkerType, ExplicitWorkerState),
}

/*  --------------  */
/* Worker Messaging */
/*  --------------  */

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MessageDci {
    ngscope_dci: NgScopeCellDci,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MessageCellInfo {
    cell_info: CellInfo,
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MessageRnti {
    rnti: u16,
}

/*  --------------  */
/*   Logic Helper   */
/*  --------------  */

pub enum AppError {
    Stopped,
    Disconnected,
}

pub fn check_not_stopped(
    rx_app_state: &mut BusReader<WorkerState>,
) -> Result<(), AppError> {
    match rx_app_state.try_recv() {
        Ok(msg) => {
            match msg {
                WorkerState::Stopped(_) => Err(AppError::Stopped),
                _ => Ok(()),
            }
        }
        Err(TryRecvError::Empty) => Ok(()),
        Err(TryRecvError::Disconnected) => Err(AppError::Disconnected),
    }
}

pub fn wait_until_running(
    rx_app_state: &mut BusReader<WorkerState>,
) -> Result<(), AppError> {
    match rx_app_state.recv() {
        Ok(msg) => match msg {
            WorkerState::Running(_) => Ok(()),
            WorkerState::Stopped(_) => Err(AppError::Stopped),
            _ => Ok(()),
        },
        Err(_) => Err(AppError::Stopped),
    }
}

pub fn send_explicit_state(tx_state: &SyncSender<WorkerState>, state: ExplicitWorkerState) -> Result<()> {
    let worker_type = match state {
        ExplicitWorkerState::Main(_) => WorkerType::Main,
        ExplicitWorkerState::Sink(_) => WorkerType::CellSink,
        ExplicitWorkerState::Source(_) => WorkerType::CellSource,
        ExplicitWorkerState::RntiMatcher(_) => WorkerType::RntiMatcher,
        ExplicitWorkerState::NgControl(_) => WorkerType::NgScopeController,
    };

    tx_state.send( WorkerState::Specific( worker_type, state))?;
    Ok(())
}
