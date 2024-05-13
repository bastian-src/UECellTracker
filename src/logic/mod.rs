use std::fmt::Debug;
use std::sync::mpsc::TryRecvError;

use bus::BusReader;

use crate::ngscope::types::NgScopeCellDci;
use crate::cell_info::CellInfo;

pub mod cell_sink;
pub mod cell_source;
pub mod ngscope_controller;
pub mod rnti_matcher;

pub const NUM_OF_WORKERS: usize = 4;

#[derive(Clone, Copy, Debug, PartialEq, Hash, Eq)]
pub enum WorkerType {
    Main,
    CellSource,
    CellSink,
    NgScopeController,
    RntiMatcher,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
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
    SpecialState,
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
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum NgControlState {
    SpecialState,
}

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq)]
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
    Stopped(BusReader<WorkerState>),
}

pub fn check_not_stopped(mut rx_app_state: BusReader<WorkerState>) -> Result<BusReader<WorkerState>, AppError> {
    match rx_app_state.try_recv() {
        Ok(msg) => {
            match msg {
                WorkerState::Stopped(_) => Err(AppError::Stopped(rx_app_state)),
                _ => Ok(rx_app_state),
            }
        },
        Err(TryRecvError::Empty) => Ok(rx_app_state),
        Err(TryRecvError::Disconnected) => Err(AppError::Stopped(rx_app_state)),
    }
}

pub fn wait_until_running(mut rx_app_state: BusReader<WorkerState>) -> Result<BusReader<WorkerState>, AppError> {
    match rx_app_state.recv() {
        Ok(msg) => {
            match msg {
                WorkerState::Running(_) => Ok(rx_app_state),
                WorkerState::Stopped(_) => Err(AppError::Stopped(rx_app_state)),
                _ => Ok(rx_app_state),
            }
        },
        Err(_) => Err(AppError::Stopped(rx_app_state)),
    }
}
