use crate::cell_info::CellInfo;
use crate::logger::log_metric;
use crate::ngscope::types::{NgScopeCellDci, NgScopeRntiDci};
use crate::parse::{Arguments, DynamicValue, FlattenedModelArgs, Scenario};
use crate::util::{print_debug, print_info};
use std::collections::{HashSet, HashMap};
use std::sync::mpsc::{SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use serde_derive::{Deserialize, Serialize};

use super::{MessageDownloadConfig, MetricA, MetricTypes};
use crate::logic::{
    check_not_stopped, wait_until_running, MainState, MessageCellInfo, MessageDci, MessageMetric,
    MessageRnti, ModelState, DEFAULT_WORKER_SLEEP_US,
};
use crate::util::determine_process_id;

pub const MAX_DCI_ARRAY_SIZE: usize = 10000;
pub const MAX_DCI_SLICE_SIZE: usize = 1000;
pub const MAX_DCI_SLICE_INDEX: usize = MAX_DCI_ARRAY_SIZE - MAX_DCI_SLICE_SIZE;
// Parameter gamma from [p. 456] PBE-CC: https://dl.acm.org/doi/abs/10.1145/3387514.3405880
pub const PHYSICAL_TO_TRANSPORT_OVERHEAD: f64 = 0.068;
pub const PHYSICAL_TO_TRANSPORT_FACTOR: f64 = 1.0 - PHYSICAL_TO_TRANSPORT_OVERHEAD;
pub const STANDARD_NOF_PRB_SLOT_TO_SUBFRAME: u64 = 2;
pub const STANDARD_BIT_PER_PRB: u64 = 500; /* Chosen from historical data */

pub const RNTI_SHARE_TYPE_ALL: u8 = 0;
pub const RNTI_SHARE_TYPE_DL_OCCURENCES: u8 = 1;
pub const RNTI_SHARE_TYPE_GREEDY: u8 = 2;
// pub const RNTI_SHARE_TYPE_UNFAIR: u8 = 0; // Don't share idle PRBs
// pub const RNTI_SHARE_TYPE_ACTIVE: u8 = 0; // Share idle PRBs among "active" RNTIs

struct DciRingBuffer {
    dci_array: Box<[NgScopeCellDci]>,
    dci_next: usize,
}

impl DciRingBuffer {
    fn new() -> DciRingBuffer {
        // Allocate the array on the heap directly
        let mut dci_vec = Vec::with_capacity(MAX_DCI_ARRAY_SIZE);
        dci_vec.resize_with(MAX_DCI_ARRAY_SIZE, NgScopeCellDci::default);

        DciRingBuffer {
            /* Allocate it on the HEAP */
            dci_array: dci_vec.into_boxed_slice(),
            dci_next: 0,
        }
    }

    fn push(&mut self, item: NgScopeCellDci) {
        if self.dci_next >= MAX_DCI_ARRAY_SIZE {
            // Copy last MAX_DCI_SLICE_SIZE items to the beginning
            let delta_index = self.dci_next - MAX_DCI_SLICE_SIZE;
            self.dci_array.copy_within(delta_index..self.dci_next, 0);
            self.dci_next = MAX_DCI_SLICE_SIZE;
        }
        self.dci_array[self.dci_next] = item;
        self.dci_next += 1;
    }

    fn slice(&self, wanted_slice_size: usize) -> &[NgScopeCellDci] {
        if wanted_slice_size == 0 || self.dci_next == 0 {
            return &[];
        }

        let slice_size = usize::min(wanted_slice_size, self.dci_next);
        let delta_index = self.dci_next - slice_size;
        &self.dci_array[delta_index..self.dci_next]
    }
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct MetricResult {
    pub transport_fair_share_capacity_bit_per_ms: u64,
    pub physical_fair_share_capacity_bit_per_ms: u64,
    pub physical_rate_bit_per_prb: u64,
    /// If 1, all RNTIs are used to determine the bit/PRB rate (more general)
    /// If 0, only the target-RNTI PRBs are used to determine the bit/PRB rate (more specific)
    pub physical_rate_coarse_flag: u8,
    pub no_tbs_prb_ratio: f64,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct MetricBasis {
    pub nof_dci: u64,
    pub p_cell: u64,
    pub nof_rnti_shared: u64,
    pub nof_rnti_in_dci: u64,
    pub rnti_share_type: u8,
    pub p_alloc: u64,
    pub p_alloc_no_tbs: u64,
    pub tbs_alloc_bit: u64,
    pub target_rnti: u16,
    pub tbs_alloc_rnti_bit: u64,
    pub p_alloc_rnti: u64,
    pub p_alloc_no_tbs_rnti: u64,
    pub p_alloc_rnti_suggested: u64,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct LogMetric {
    result: MetricResult,
    basis: MetricBasis,
}

pub struct ModelHandlerArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_model_state: SyncSender<ModelState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub rx_download_config: BusReader<MessageDownloadConfig>,
    pub tx_metric: Bus<MessageMetric>,
}

struct RunArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_model_state: SyncSender<ModelState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub rx_download_config: BusReader<MessageDownloadConfig>,
    pub tx_metric: Bus<MessageMetric>,
}

struct RunParameters<'a> {
    tx_metric: &'a mut Bus<MessageMetric>,
    dci_buffer: &'a DciRingBuffer,
    rnti: u16,
    cell_info: &'a CellInfo,
    is_log_metric: &'a bool,
}

struct RunParametersSendingBehavior<'a> {
    metric_sending_interval_us: &'a mut u64,
    metric_smoothing_size_ms: &'a mut u64,
    last_metric_timestamp_us: &'a mut u64,
    rnti_share_type: &'a u8,
    last_rtt_us: &'a Option<u64>,
    model_args: &'a FlattenedModelArgs,
}

pub fn deploy_model_handler(args: ModelHandlerArgs) -> Result<JoinHandle<()>> {
    let mut run_args = RunArgs {
        app_args: args.app_args,
        rx_app_state: args.rx_app_state,
        tx_model_state: args.tx_model_state,
        rx_cell_info: args.rx_cell_info,
        rx_dci: args.rx_dci,
        rx_rnti: args.rx_rnti,
        rx_download_config: args.rx_download_config,
        tx_metric: args.tx_metric,
    };

    let builder = thread::Builder::new().name("[model]".to_string());
    let thread = builder.spawn(move || {
        let _ = run(&mut run_args);
        finish(run_args);
    })?;
    Ok(thread)
}

fn send_final_state(tx_model_state: &SyncSender<ModelState>) -> Result<()> {
    Ok(tx_model_state.send(ModelState::Stopped)?)
}

fn wait_for_running(rx_app_state: &mut BusReader<MainState>) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => Err(anyhow!("[model] Main did not send 'Running' message")),
    }
}

fn run(run_args: &mut RunArgs) -> Result<()> {
    let app_args = &run_args.app_args;
    let rx_app_state: &mut BusReader<MainState> = &mut run_args.rx_app_state;
    let tx_model_state: &mut SyncSender<ModelState> = &mut run_args.tx_model_state;
    let rx_cell_info: &mut BusReader<MessageCellInfo> = &mut run_args.rx_cell_info;
    let rx_dci: &mut BusReader<MessageDci> = &mut run_args.rx_dci;
    let rx_rnti: &mut BusReader<MessageRnti> = &mut run_args.rx_rnti;
    let rx_download_config: &mut BusReader<MessageDownloadConfig> =
        &mut run_args.rx_download_config;
    let tx_metric: &mut Bus<MessageMetric> = &mut run_args.tx_metric;

    tx_model_state.send(ModelState::Running)?;
    wait_for_running(rx_app_state)?;
    print_info(&format!("[model]: \t\tPID {:?}", determine_process_id()));
    let sleep_duration = Duration::from_micros(DEFAULT_WORKER_SLEEP_US);

    let model_args = FlattenedModelArgs::from_unflattened(app_args.clone().model.unwrap())?;
    let scenario = app_args.scenario.unwrap();

    let is_log_metric: bool = model_args.model_log_metric;
    let mut last_metric_timestamp_us: u64 = chrono::Local::now().timestamp_micros() as u64;
    let mut dci_buffer = DciRingBuffer::new();
    let mut last_rnti: Option<u16> = None;
    let mut last_cell_info: Option<CellInfo> = None;
    let mut last_rnti_share_type: u8 = RNTI_SHARE_TYPE_ALL;
    let mut last_rtt_us: Option<u64> = Some(40000);
    let mut metric_sending_interval_us: u64 = determine_sending_interval(&model_args, &last_rtt_us);
    let mut metric_smoothing_size_ms: u64 = determine_smoothing_size(&model_args, &last_rtt_us);

    loop {
        /* <precheck> */
        thread::sleep(sleep_duration);
        if check_not_stopped(rx_app_state).is_err() {
            break;
        }
        match rx_dci.try_recv() {
            Ok(dci) => {
                dci_buffer.push(dci.ngscope_dci);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        match rx_cell_info.try_recv() {
            Ok(cell_info) => last_cell_info = Some(cell_info.cell_info.clone()),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        match rx_rnti.try_recv() {
            Ok(rnti_msg) => {
                if let Some(rnti) = rnti_msg.cell_rnti.values().copied().next() {
                    last_rnti = Some(rnti);
                    print_debug(&format!(
                        "DEBUG [model] new rnti {:#?}",
                        rnti_msg.cell_rnti.get(&0).unwrap()
                    ));
                }
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        match rx_download_config.try_recv() {
            Ok(download_config) => {
                last_rtt_us = Some(download_config.config.rtt_us);
                last_rnti_share_type = download_config.config.rnti_share_type;
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        if is_idle_scenario(scenario) {
            continue;
        }
        /* </precheck> */

        if let (Some(rnti), Some(cell_info)) = (last_rnti, last_cell_info.clone()) {
            let delta_last_metric_sent_us =
                chrono::Local::now().timestamp_micros() as u64 - last_metric_timestamp_us;
            if delta_last_metric_sent_us > metric_sending_interval_us {
                let mut run_params = RunParameters {
                    tx_metric,
                    dci_buffer: &dci_buffer,
                    rnti,
                    cell_info: &cell_info,
                    is_log_metric: &is_log_metric,
                };

                let mut sending_behavior = RunParametersSendingBehavior {
                    metric_sending_interval_us: &mut metric_sending_interval_us,
                    metric_smoothing_size_ms: &mut metric_smoothing_size_ms,
                    last_metric_timestamp_us: &mut last_metric_timestamp_us,
                    rnti_share_type: &last_rnti_share_type,
                    model_args: &model_args,
                    last_rtt_us: &last_rtt_us,
                };
                handle_calculate_metric(&mut run_params, &mut sending_behavior);
            }
        }
    }

    Ok(())
}

fn is_idle_scenario(scenario: Scenario) -> bool {
    match scenario {
        Scenario::TrackCellDciOnly => true,
        Scenario::TrackUeAndEstimateTransportCapacity => false,
        Scenario::PerformMeasurement => false,
    }
}

fn finish(run_args: RunArgs) {
    let _ = send_final_state(&run_args.tx_model_state);
}

fn handle_calculate_metric(
    run_params: &mut RunParameters,
    sending_behavior: &mut RunParametersSendingBehavior,
) {
    // Use the fields from the structs
    let RunParameters {
        tx_metric,
        dci_buffer,
        rnti,
        cell_info,
        is_log_metric,
    } = run_params;

    let RunParametersSendingBehavior {
        metric_sending_interval_us,
        metric_smoothing_size_ms,
        last_metric_timestamp_us,
        rnti_share_type,
        model_args,
        last_rtt_us,
    } = sending_behavior;

    let buffer_slice_size: usize = **metric_smoothing_size_ms as usize;
    let buffer_slice = dci_buffer.slice(buffer_slice_size);
    if !buffer_slice.is_empty() {
        if let Ok(metric_wrapper) = calculate_capacity(
            *rnti,
            cell_info,
            buffer_slice,
            is_log_metric,
            rnti_share_type,
        ) {
            let transport_capacity = metric_wrapper
                .result
                .transport_fair_share_capacity_bit_per_ms;
            let physical_rate_flag = metric_wrapper.result.physical_rate_coarse_flag;
            let physical_rate = metric_wrapper.result.physical_rate_bit_per_prb;
            let no_tbs_prb_ratio = metric_wrapper.result.no_tbs_prb_ratio;
            let now_us = chrono::Local::now().timestamp_micros() as u64;

            tx_metric.broadcast(MessageMetric {
                metric: MetricTypes::A(MetricA {
                    timestamp_us: now_us,
                    fair_share_send_rate: transport_capacity,
                    latest_dci_timestamp_us: buffer_slice.first().unwrap().time_stamp,
                    oldest_dci_timestamp_us: buffer_slice.last().unwrap().time_stamp,
                    nof_dci: buffer_slice.len() as u16,
                    no_tbs_prb_ratio,
                    flag_phy_rate_all_rnti: physical_rate_flag,
                    phy_rate: physical_rate,
                }),
            });
        }
        **last_metric_timestamp_us = chrono::Local::now().timestamp_micros() as u64;
        **metric_sending_interval_us = determine_sending_interval(model_args, last_rtt_us);
        **metric_smoothing_size_ms = determine_smoothing_size(model_args, last_rtt_us);
    } else {
        print_debug("DEBUG [model] skipping metric calculation, dci slice is 0");
    }
}

fn calculate_capacity(
    target_rnti: u16,
    cell_info: &CellInfo,
    dci_list: &[NgScopeCellDci],
    is_log_metric: &bool,
    rnti_share_type: &u8,
) -> Result<LogMetric> {
    let metric_wrapper =
        calculate_pbe_cc_capacity(target_rnti, cell_info, dci_list, rnti_share_type)?;
    if *is_log_metric {
        let _ = log_metric(metric_wrapper.clone());
    }
    print_debug(&format!(
        "DEBUG [model] model:
                                 c_t:      \t{:6?} bit/ms | {:3.3?} Mbit/s
                                 c_p:      \t{:6?} bit/ms | {:3.3?} Mbit/s
                                 phy rate: \t{:6?} bit/PRB
                                 phy flag: \t{:?}
                                 no_tbs %: \t{:?}",
        metric_wrapper
            .result
            .transport_fair_share_capacity_bit_per_ms,
        metric_wrapper
            .result
            .transport_fair_share_capacity_bit_per_ms as f64
            * 1000.0
            / (1024.0 * 1024.0),
        metric_wrapper
            .result
            .physical_fair_share_capacity_bit_per_ms,
        metric_wrapper
            .result
            .physical_fair_share_capacity_bit_per_ms as f64
            * 1000.0
            / (1024.0 * 1024.0),
        metric_wrapper.result.physical_rate_bit_per_prb,
        metric_wrapper.result.physical_rate_coarse_flag,
        metric_wrapper.result.no_tbs_prb_ratio,
    ));
    Ok(metric_wrapper)
}

/*
 * PHY-Layer fair-share capacity in bit/ms in downlink
 * (capacity, bit/PRB ratio, re-transmissions)
 *
 * According to PBE-CC: https://dl.acm.org/doi/abs/10.1145/3387514.3405880
 * */
fn calculate_pbe_cc_capacity(
    target_rnti: u16,
    cell_info: &CellInfo,
    dci_list: &[NgScopeCellDci],
    rnti_share_type: &u8,
) -> Result<LogMetric> {
    let nof_dci: u64 = dci_list.len() as u64;
    if nof_dci == 0 {
        return Err(anyhow!("Cannot calculate capacity with 0 DCI"));
    }

    /*
     * Determine parameters of the given DCIs
     * */
    // Total number of PRBs in a subframe, that the cell can offer
    let p_cell: u64 =
        STANDARD_NOF_PRB_SLOT_TO_SUBFRAME * cell_info.cells[0].nof_prb as u64 * nof_dci;

    // Total number of unique RNTIs
    let nof_rnti: u64 = dci_list
        .iter()
        .flat_map(|dci| {
            dci.rnti_list
                .iter()
                .take(dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.dl_prb > 0)
                .map(|rnti_dci| rnti_dci.rnti)
        })
        .collect::<HashSet<u16>>()
        .len() as u64;

    // Number of allocated PRBs that contain TBS information
    let p_alloc: u64 = dci_list.iter().map(|dci| dci.total_dl_prb as u64).sum();
    // Number of allocated PRBs that contain no TBS information
    let p_alloc_no_tbs: u64 = dci_list
        .iter()
        .map(|dci| dci.total_dl_no_tbs_prb as u64)
        .sum();

    // Total decoded traffic in bit
    let tbs_alloc_bit: u64 = dci_list.iter().map(|dci| dci.total_dl_tbs_bit).sum();

    // The DCIs of the target RNTI (our UE)
    let target_rnti_dci_list: Vec<&NgScopeRntiDci> = dci_list
        .iter()
        .flat_map(|dci| {
            dci.rnti_list
                .iter()
                .take(dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.rnti == target_rnti)
                .filter(|rnti_dci| rnti_dci.dl_prb > 0)
        })
        .collect::<Vec<&NgScopeRntiDci>>();

    // The traffic of our RNTI in bit
    let tbs_alloc_rnti_bit: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_tbs_bit as u64)
        .sum::<u64>();

    // The number of allocated PRBs by our RNTI (with TBS)
    let p_alloc_rnti: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_prb as u64)
        .sum::<u64>();

    // The number of allocated PRBs by our RNTI (without TBS -> traffic in bits unknown)
    let p_alloc_no_tbs_rnti: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_no_tbs_prb as u64)
        .sum::<u64>();

    // Total number of allocated PRBs in the given DCIs
    let p_alloc_total = p_alloc + p_alloc_no_tbs;

    // Flag to signialize that the bit per PRB rate does NOT belong to our RNTI and is a coarse
    // estimation
    let mut r_w_coarse_flag: u8 = 0;

    // [bit/PRB]
    let r_w: u64 = if p_alloc_rnti == 0 {
        r_w_coarse_flag = 1;
        if p_alloc > 0 {
            /* Use mean ratio of other RNTIs */
            tbs_alloc_bit / p_alloc
        } else {
            /* Use bit per PRB rate from experience */
            STANDARD_BIT_PER_PRB
        }
    } else {
        /* Use the bit per PRB of our RNTI */
        tbs_alloc_rnti_bit / p_alloc_rnti
    };

    /* Number of unused PRBs in the given DCI-timeframe */
    let p_idle: u64 = match p_cell.checked_sub(p_alloc_total) {
        Some(result_p_idle) => result_p_idle,
        None => return Err(anyhow!("error in calculate PBE capacity: p_idle < 0! (probably more p_alloc_no_tbs than p_cell)")),
    };

    /*
     * Determine with how many RNTIs the idle PRBs shall be shared (RNTI fair share type)
     * */
    let mut used_rnti_share_type = *rnti_share_type;
    let nof_rnti_shared: u64 = match *rnti_share_type {
        RNTI_SHARE_TYPE_DL_OCCURENCES => {
            let nof_occurenes_threshould = nof_dci / 10;
            let rnti_counts: HashMap<u16, u64> = dci_list.iter()
                .flat_map(|dci| {
                    dci.rnti_list
                        .iter()
                        .take(dci.nof_rnti as usize)
                        .filter(|rnti_dci| rnti_dci.dl_prb > 0)
                        .map(|rnti_dci| rnti_dci.rnti)
                })
                .fold(HashMap::new(), |mut acc, rnti| {
                    *acc.entry(rnti).or_insert(0) += 1;
                    acc
                });
            let nof_all_rnti = rnti_counts.len();
            let mut nof_filtered = rnti_counts.into_iter()
                .filter(|&(_, count)| count >= nof_occurenes_threshould)
                .collect::<Vec<(u16, u64)>>()
                .len() as u64;
            if nof_filtered == 0 {
                nof_filtered = 1;
            }
            print_debug(&format!("DEBUG [model] RNTI Fair Share Type 1: {} -> {} | {}", nof_all_rnti, nof_filtered, nof_occurenes_threshould));
            nof_filtered
        }
        RNTI_SHARE_TYPE_GREEDY => {
            1
        }
        // Default: RNTI_SHARE_TYPE_ALL
        _ => {
            used_rnti_share_type = RNTI_SHARE_TYPE_ALL;
            if nof_rnti <= 1 {
                1
            } else {
                nof_rnti
            }
        }
    };

    /*
     * Determine the fair share badnwidth c_p (physical layer) and c_t (transport layer)
     * */
    let p_alloc_rnti_suggested: u64 =
        p_alloc_rnti + ((p_idle + nof_rnti_shared - 1) / nof_rnti_shared);
    let c_p: u64 =
        ((r_w as f64 * (p_alloc_rnti + (p_alloc_rnti_suggested)) as f64) / nof_dci as f64) as u64;
    let c_t = translate_physcial_to_transport_simple(c_p);

    let mut no_tbs_prb_ratio = 0.0;
    if p_alloc_no_tbs > 0 {
        no_tbs_prb_ratio =  p_alloc_no_tbs as f64 / p_alloc_total as f64;
    }

    print_debug(&format!(
        "DEBUG [model] parameters:
                         nof_dci:                {:8?}
                         p_cell:                 {:8?}
                         nof_rnti:               {:8?}
                         p_alloc:                {:8?} | {:3?} PRB/DCI
                         p_alloc_no_tbs:         {:8?} | {:3?} PRB/DCI
                         p_alloc_rnti:           {:8?} | {:3?} PRB/DCI
                         p_alloc_rnti_suggested: {:8?} | {:3?} PRB/DCI
                         p_alloc_rnti_no_tbs:    {:8?}
                         tbs_alloc_rnti:         {:8?}
                         RNTI MB/s:              {:8.3?}
                         RNTI Mb/s:              {:8.3?}",
        nof_dci,
        p_cell,
        nof_rnti,
        p_alloc,
        p_alloc / nof_dci,
        p_alloc_no_tbs,
        p_alloc_no_tbs / nof_dci,
        p_alloc_rnti,
        p_alloc_rnti / nof_dci,
        p_alloc_rnti_suggested,
        p_alloc_rnti_suggested / nof_dci,
        p_alloc_no_tbs_rnti,
        tbs_alloc_rnti_bit,
        (tbs_alloc_rnti_bit as f64 / (8.0 * nof_dci as f64)) * 1000.0 / (1024.0 * 1024.0),
        (tbs_alloc_rnti_bit as f64 / (nof_dci as f64)) * 1000.0 / (1024.0 * 1024.0),
    ));

    Ok(LogMetric {
        result: MetricResult {
            physical_fair_share_capacity_bit_per_ms: c_p,
            transport_fair_share_capacity_bit_per_ms: c_t,
            physical_rate_bit_per_prb: r_w,
            physical_rate_coarse_flag: r_w_coarse_flag,
            no_tbs_prb_ratio,
        },
        basis: MetricBasis {
            nof_dci,
            p_cell,
            nof_rnti_shared,
            nof_rnti_in_dci: nof_rnti,
            rnti_share_type: used_rnti_share_type,
            p_alloc,
            p_alloc_no_tbs,
            tbs_alloc_bit,
            target_rnti,
            tbs_alloc_rnti_bit,
            p_alloc_rnti,
            p_alloc_rnti_suggested,
            p_alloc_no_tbs_rnti,
        },
    })
}

fn translate_physcial_to_transport_simple(c_physical: u64) -> u64 {
    (c_physical as f64 * PHYSICAL_TO_TRANSPORT_FACTOR) as u64
}

fn determine_sending_interval(model_args: &FlattenedModelArgs, last_rtt_us: &Option<u64>) -> u64 {
    match model_args.model_send_metric_interval_type {
        DynamicValue::FixedMs => model_args.model_send_metric_interval_value as u64 * 1000,
        DynamicValue::RttFactor => {
            (last_rtt_us.unwrap() as f64 * model_args.model_send_metric_interval_value) as u64
        }
    }
}

fn determine_smoothing_size(model_args: &FlattenedModelArgs, last_rtt_us: &Option<u64>) -> u64 {
    let unbound_slice = match model_args.model_metric_smoothing_size_type {
        DynamicValue::FixedMs => model_args.model_metric_smoothing_size_value as u64,
        DynamicValue::RttFactor => {
            (last_rtt_us.unwrap() as f64 * model_args.model_metric_smoothing_size_value / 1000.0)
                as u64
        }
    };
    if unbound_slice > MAX_DCI_SLICE_SIZE as u64 {
        return MAX_DCI_SLICE_SIZE as u64;
    }
    unbound_slice
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        cell_info::SingleCell,
        ngscope::types::{NgScopeRntiDci, NGSCOPE_MAX_NOF_RNTI},
    };

    fn dummy_rnti_dci(nof_rnti: u8) -> [NgScopeRntiDci; NGSCOPE_MAX_NOF_RNTI] {
        let mut rnti_list = [NgScopeRntiDci::default(); NGSCOPE_MAX_NOF_RNTI];
        for i in 0..nof_rnti {
            rnti_list[0].dl_tbs_bit = 1024;
            rnti_list[0].dl_prb = 2 + i;
            rnti_list[0].rnti = 123 + i as u16;
        }

        rnti_list
    }

    fn dummy_dci_slice() -> Vec<NgScopeCellDci> {
        vec![
            NgScopeCellDci {
                nof_rnti: 2,
                total_dl_prb: 5,
                rnti_list: dummy_rnti_dci(2),
                ..Default::default()
            },
            NgScopeCellDci {
                nof_rnti: 1,
                total_dl_prb: 2,
                rnti_list: dummy_rnti_dci(1),
                ..Default::default()
            },
            NgScopeCellDci {
                nof_rnti: 4,
                total_dl_prb: 14,
                rnti_list: dummy_rnti_dci(4),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn test_capacity() -> Result<()> {
        let dummy_rnti: u16 = 123;
        let dummy_cell_info = CellInfo {
            cells: vec![SingleCell {
                nof_prb: 100,
                ..Default::default()
            }],
        };
        let metric_params = calculate_capacity(
            dummy_rnti,
            &dummy_cell_info,
            &dummy_dci_slice(),
            &false,
            &RNTI_SHARE_TYPE_ALL,
        )?;
        assert_eq!(
            metric_params.result.physical_fair_share_capacity_bit_per_ms,
            33621
        );
        assert_eq!(metric_params.result.physical_rate_bit_per_prb, 512);
        assert_eq!(metric_params.result.physical_rate_coarse_flag, 0);
        assert_eq!(metric_params.result.no_tbs_prb_ratio, 0.0);
        Ok(())
    }
}
