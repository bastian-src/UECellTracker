#![allow(dead_code)]

use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::util::print_info;
use anyhow::{anyhow, Result};
use bus::BusReader;

use crate::cell_info::CellInfo;
use crate::logic::rnti_matcher::TrafficCollection;
use crate::ngscope::config::NgScopeConfig;
use crate::ngscope::types::{NgScopeCellDci, NgScopeCellConfig};

use self::downloader::{DownloadConfig, DownloadFinishParameters};

pub mod cell_source;
pub mod downloader;
pub mod model_handler;
pub mod ngscope_controller;
pub mod rnti_matcher;
pub mod traffic_patterns;

pub const NUM_OF_WORKERS: usize = 4;
pub const DEFAULT_WORKER_SLEEP_MS: u64 = 2;
pub const DEFAULT_WORKER_SLEEP_US: u64 = 50;
pub const WORKER_SLEEP_LONG_MS: u64 = 500;
pub const CHANNEL_SYNC_SIZE: usize = 10;
pub const BUS_SIZE_APP_STATE: usize = 50;
pub const BUS_SIZE_DCI: usize = 100000;
pub const BUS_SIZE_CELL_INFO: usize = 100;
pub const BUS_SIZE_RNTI: usize = 100;
pub const BUS_SIZE_METRIC: usize = 100;

pub trait WorkerState: Sized + Clone + Sync + Debug {
    fn to_general_state(&self) -> GeneralState;
    fn worker_name() -> String;
}

pub trait WorkerChannel<T: WorkerState> {
    fn worker_try_recv(&self) -> Result<Option<T>, TryRecvError>;
    fn worker_print_on_recv(&self) -> Result<Option<T>, TryRecvError>;
    fn worker_try_recv_general_state(&self) -> Result<Option<GeneralState>>;
    fn worker_recv_general_state(&self) -> Result<GeneralState>;
}

impl<T: WorkerState> WorkerChannel<T> for Receiver<T> {
    fn worker_try_recv(&self) -> Result<Option<T>, TryRecvError> {
        match self.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(TryRecvError::Disconnected),
        }
    }

    fn worker_print_on_recv(&self) -> Result<Option<T>, TryRecvError> {
        match self.worker_try_recv() {
            Ok(Some(msg)) => {
                print_info(&format!(
                    "[main] message from {}: {:#?}",
                    T::worker_name(),
                    msg
                ));
                Ok(Some(msg))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn worker_recv_general_state(&self) -> Result<GeneralState> {
        Ok(self.recv()?.to_general_state())
    }

    fn worker_try_recv_general_state(&self) -> Result<Option<GeneralState>> {
        match self.try_recv() {
            Ok(msg) => Ok(Some(msg.to_general_state())),
            Err(TryRecvError::Empty) => Ok(None),
            Err(TryRecvError::Disconnected) => Err(TryRecvError::Disconnected.into()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GeneralState {
    Running,
    Stopped,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MainState {
    Running,
    Stopped,
    NotifyStop,
    UeConnectionReset, /* NgScope has been restarted */
}

impl WorkerState for MainState {
    fn worker_name() -> String {
        "main".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            MainState::Running => GeneralState::Running,
            MainState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModelState {
    Running,
    Stopped,
    SpecialState,
}

impl WorkerState for ModelState {
    fn worker_name() -> String {
        "model".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            ModelState::Running => GeneralState::Running,
            ModelState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SourceState {
    Running,
    Stopped,
    SpecialState,
}

impl WorkerState for SourceState {
    fn worker_name() -> String {
        "source".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            SourceState::Running => GeneralState::Running,
            SourceState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RntiMatcherState {
    Running,
    Stopped,
    Idle,
    StartMatching,
    MatchingCollectDci(Box<TrafficCollection>),
    MatchingProcessDci(Box<TrafficCollection>),
    MatchingPublishRnti(MessageRnti),
    MatchingError(RntiMatchingErrorType),
    StoppingTrafficGeneratorThread,
    SleepMs(u64, Box<RntiMatcherState>),
}

impl RntiMatcherState {
    fn name(&self) -> String {
        let name_str = match self {
            RntiMatcherState::Running => "Running",
            RntiMatcherState::Stopped => "Stopped",
            RntiMatcherState::Idle => "Idle",
            RntiMatcherState::StartMatching => "StartMatching",
            RntiMatcherState::MatchingCollectDci(_) => "MatchingCollectDci",
            RntiMatcherState::MatchingProcessDci(_) => "MatchingProcessDci",
            RntiMatcherState::MatchingPublishRnti(_) => "MatchingPublishRnti",
            RntiMatcherState::MatchingError(_) => "MatchingError",
            RntiMatcherState::StoppingTrafficGeneratorThread => "StoppingTrafficGeneratorThread",
            RntiMatcherState::SleepMs(_, _) => "Sleep",
        };
        name_str.to_owned()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum RntiMatchingErrorType {
    ExceededDciTimestampDelta,
    ErrorGeneratingTrafficPatternFeatures,
    ErrorFindingBestMatchingRnti,
}

impl WorkerState for RntiMatcherState {
    fn worker_name() -> String {
        "rntimatcher".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            RntiMatcherState::Running => GeneralState::Running,
            RntiMatcherState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum NgControlState {
    Running,
    Stopped,
    CheckingCellInfo,
    StartNgScope(Box<NgScopeConfig>),
    StopNgScope,
    TriggerListenDci,
    WaitForTriggerResponse,
    SuccessfulTriggerResponse,
    SleepMs(u64, Box<NgControlState>),
    StoppingDciFetcherThread,
    RestartingNgScopeProcess,
    StoppingNgScopeProcess,
}

impl WorkerState for NgControlState {
    fn worker_name() -> String {
        "ngcontrol".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            NgControlState::Running => GeneralState::Running,
            NgControlState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DownloaderState {
    Stopped,
    Ready,
    SleepMs(u64, Box<DownloaderState>),
    StartDownload,
    ErrorStartingDownload(String),
    Downloading,
    PostDownload,
    FinishDownload(DownloadFinishParameters),
}

impl WorkerState for DownloaderState {
    fn worker_name() -> String {
        "download".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            DownloaderState::Ready => GeneralState::Running,
            DownloaderState::SleepMs(_, _) => GeneralState::Running,
            DownloaderState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

/*  --------------  */
/* Worker Messaging */
/*  --------------  */

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub enum MessageDci {
    CellDci(Box<NgScopeCellDci>),
    CellConfig(Box<NgScopeCellConfig>),
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct MessageCellInfo {
    cell_info: CellInfo,
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Default)]
pub struct MessageRnti {
    /* cell_id -> ue_rnti */
    cell_rnti: HashMap<u64, u16>,
}

/* Wrapping messages */
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct MessageMetric {
    metric: MetricTypes,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum MetricTypes {
    A(MetricA),
}

#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq)]
pub struct MessageDownloadConfig {
    config: DownloadConfig,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MetricA {
    /// Timestamp when the metric was calculated
    timestamp_us: u64,
    /// Fair share send rate [bits/subframe] = [bits/ms]
    fair_share_send_rate: u64,
    /// Timestamp of the latest DCI used to calculate the metric
    latest_dci_timestamp_us: u64,
    /// Timestamp of the oldest DCI used to calculate the metric
    oldest_dci_timestamp_us: u64,
    /// Number of DCIs used to calculate the metric
    nof_dci: u16,
    /// Ratio of no TBS PRBs to PRBs with TBS (~reTx ratio)
    no_tbs_prb_ratio: f64,
    /// Flag, signalling whether phy_rate was averagerd over all RNTIs or just our UE RNTI
    flag_phy_rate_all_rnti: u8,
    /// Average bit per PRB (either over all RNTIs or just the UE RNTI)
    phy_rate: u64,
}

/*  --------------  */
/*   Logic Helper   */
/*  --------------  */

pub enum AppError {
    Stopped,
    Disconnected,
}

pub fn check_not_stopped<T: WorkerState>(rx_state: &mut BusReader<T>) -> Result<Option<T>> {
    match rx_state.try_recv() {
        Ok(msg) => match msg.to_general_state() {
            GeneralState::Stopped => Err(anyhow!("BusReader received GeneralState::Stopped!")),
            _ => Ok(Some(msg)),
        },
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(anyhow!("BusReader disconnected!")),
    }
}

pub fn wait_until_running<T: WorkerState>(rx_state: &mut BusReader<T>) -> Result<()> {
    match rx_state.recv() {
        Ok(msg) => match msg.to_general_state() {
            GeneralState::Running => Ok(()),
            GeneralState::Stopped => Err(anyhow!("BusReader received GeneralState::Stopped!")),
            GeneralState::Unknown => Ok(()),
        },
        Err(_) => Err(anyhow!("BusReader disconnected!")),
    }
}
