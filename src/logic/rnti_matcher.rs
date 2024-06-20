#![allow(dead_code)]

use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use serde_derive::{Deserialize, Serialize};
use nalgebra::{DMatrix, DVector};

use crate::logic::traffic_patterns::{TrafficPattern, TrafficPatternFeatures};
use crate::logic::{
    check_not_stopped, wait_until_running, MainState, MessageDci, MessageRnti, RntiMatcherState,
    RntiMatchingErrorType, CHANNEL_SYNC_SIZE, DEFAULT_WORKER_SLEEP_MS,
};
use crate::ngscope::types::NgScopeCellDci;
use crate::parse::{Arguments, FlattenedRntiMatchingArgs};

use crate::util::{
    CellRntiRingBuffer,
    log_rnti_matching_traffic, print_debug, print_info, determine_process_id,
};

use crate::math_util::{
    calculate_mean_variance, calculate_median, calculate_weighted_euclidean_distance,
    calculate_weighted_euclidean_distance_matrix, standardize_feature_vec,
};


pub const MATCHING_INTERVAL_MS: u64 = 1000;
pub const MATCHING_TRAFFIC_PATTERN_TIME_OVERLAP_FACTOR: f64 = 1.1;
pub const MATCHING_MAX_DCI_TIMESTAMP_DELTA_MS: u64 = 100;
pub const MATCHING_UL_BYTES_LOWER_BOUND_FACTOR: f64 = 0.5;
pub const MATCHING_UL_BYTES_UPPER_BOUND_FACTOR: f64 = 4.0;
pub const TIME_MS_TO_US_FACTOR: u64 = 1000;
pub const MATCHING_LOG_FILE_PREFIX: &str = "./.logs/rnti_matching_pattern_";
pub const MATCHING_LOG_FILE_SUFFIX: &str = ".jsonl";
pub const COLLECT_DCI_MAX_TIMESTAMP_DELTA_US: u64 = 50000;

pub const BASIC_FILTER_MAX_TOTAL_UL_FACTOR: f64 = 100.0;
pub const BASIC_FILTER_MIN_TOTAL_UL_FACTOR: f64 = 0.005;
pub const BASIC_FILTER_MAX_UL_PER_DCI: u64 = 5_000_000;
pub const BASIC_FILTER_MIN_OCCURENCES_FACTOR: f64 = 0.005;

pub const RNTI_RING_BUFFER_SIZE: usize = 5;


/*
 * Feature vector, order matters:
 *
 * DCI count (occurences)
 * Total UL bytes
 * UL bytes median
 * UL bytes mean
 * UL bytes variance
 * DCI timestamp delta median
 * DCI timestamp delta mean
 * DCI timestamp delta variance
 * */
// pub const MATCHING_WEIGHTINGS: [f64; 8] = [
//     0.5,    /* DCI count (occurences) */
//     0.1,    /* Total UL bytes */
//     0.15,    /* UL bytes median */
//     0.025,  /* UL bytes mean */
//     0.025,  /* UL bytes variance */
//     0.15,    /* DCI time delta median */
//     0.025,  /* DCI time delta mean */
//     0.025,  /* DCI time delta variance */
// ];

/* on D, not so nice */
// pub const MATCHING_WEIGHTINGS: [f64; 8] = [
//     0.3,    /* DCI count (occurences) */
//     0.3,    /* Total UL bytes */
//     0.1,    /* UL bytes median */
//     0.2,  /* UL bytes mean */
//     0.025,  /* UL bytes variance */
//     0.025,    /* DCI time delta median */
//     0.025,  /* DCI time delta mean */
//     0.025,  /* DCI time delta variance */
// ];

pub const MATCHING_WEIGHTINGS: [f64; 8] = [
    0.5,    /* DCI count (occurences) */
    0.3,    /* Total UL bytes */
    0.1,    /* UL bytes median */
    0.020,  /* UL bytes mean */
    0.020,  /* UL bytes variance */
    0.020,    /* DCI time delta median */
    0.020,  /* DCI time delta mean */
    0.020,  /* DCI time delta variance */
];

#[derive(Clone, Debug, PartialEq)]
enum LocalGeneratorState {
    Stop,
    SendPattern(String, Box<TrafficPattern>),
    PatternSent,
    Idle,
}

pub struct RntiMatcherArgs {
    pub rx_app_state: BusReader<MainState>,
    pub tx_rntimatcher_state: SyncSender<RntiMatcherState>,
    pub app_args: Arguments,
    pub rx_dci: BusReader<MessageDci>,
    pub tx_rnti: Bus<MessageRnti>,
}

struct RunArgs {
    rx_app_state: BusReader<MainState>,
    tx_rntimatcher_state: SyncSender<RntiMatcherState>,
    app_args: Arguments,
    rx_dci: BusReader<MessageDci>,
    tx_rnti: Bus<MessageRnti>,
    tx_gen_thread_handle: Option<SyncSender<LocalGeneratorState>>,
    gen_thread_handle: Option<JoinHandle<()>>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TrafficCollection {
    /* cell_id -> { traffic } */
    pub cell_traffic: HashMap<u64, CellTrafficCollection>,
    pub start_timestamp_ms: u64,
    pub finish_timestamp_ms: u64,
    pub traffic_pattern_features: TrafficPatternFeatures,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct CellTrafficCollection {
    /* rnti -> { {tx, tx+1, tx+2} }*/
    pub traffic: HashMap<u16, UeTraffic>,
    pub nof_total_dci: u64,
    pub nof_empty_dci: u64,
    pub first_dci_timestamp_us: u64,
    pub last_dci_timestamp_us: u64,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct UeTraffic {
    /* tx -> { dl, ul }*/
    pub traffic: HashMap<u64, Traffic>,
    pub total_dl_bytes: u64,
    pub total_ul_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Traffic {
    pub dl_bytes: u64,
    pub ul_bytes: u64,
}

pub fn deploy_rnti_matcher(args: RntiMatcherArgs) -> Result<JoinHandle<()>> {
    let mut run_args: RunArgs = RunArgs {
        rx_app_state: args.rx_app_state,
        tx_rntimatcher_state: args.tx_rntimatcher_state,
        app_args: args.app_args,
        rx_dci: args.rx_dci,
        tx_rnti: args.tx_rnti,
        tx_gen_thread_handle: None,
        gen_thread_handle: None,
    };

    let thread = thread::spawn(move || {
        let _ = run(&mut run_args);
        finish(run_args);
    });
    Ok(thread)
}

fn run(run_args: &mut RunArgs) -> Result<()> {
    let rx_app_state = &mut run_args.rx_app_state;
    let tx_rntimatcher_state = &mut run_args.tx_rntimatcher_state;
    let app_args = &run_args.app_args;
    let rx_dci = &mut run_args.rx_dci;
    let tx_rnti = &mut run_args.tx_rnti;

    tx_rntimatcher_state.send(RntiMatcherState::Running)?;
    wait_for_running(rx_app_state, tx_rntimatcher_state)?;
    print_info(&format!(
        "[rntimatcher]: \t\tPID {:?}",
        determine_process_id()
    ));

    let matching_args =
        FlattenedRntiMatchingArgs::from_unflattened(app_args.clone().rntimatching.unwrap())?;

    let (tx_gen_thread, rx_gen_thread) = sync_channel::<LocalGeneratorState>(CHANNEL_SYNC_SIZE);
    run_args.gen_thread_handle = Some(deploy_traffic_generator_thread(
        rx_gen_thread,
        matching_args.matching_local_addr,
    )?);
    run_args.tx_gen_thread_handle = Some(tx_gen_thread.clone());

    let mut cell_rnti_ring_buffer: CellRntiRingBuffer = CellRntiRingBuffer::new(RNTI_RING_BUFFER_SIZE);
    let traffic_destination = matching_args.matching_traffic_destination;
    let traffic_pattern = matching_args.matching_traffic_pattern.generate_pattern();
    let matching_log_file_path = &format!(
        "{}{:?}{}",
        MATCHING_LOG_FILE_PREFIX, matching_args.matching_traffic_pattern, MATCHING_LOG_FILE_SUFFIX
    );
    let mut matcher_state: RntiMatcherState = RntiMatcherState::Idle;

    loop {
        /* <precheck> */
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        match check_not_stopped(rx_app_state) {
            Ok(Some(MainState::UeConnectionReset)) => {
                matcher_state = RntiMatcherState::StartMatching;
            }
            Err(_) => break,
            _ => {}
        }
        /* unpack dci at every iteration to keep the queue "empty"! */
        let latest_dcis = collect_dcis(rx_dci);
        /* </precheck> */

        matcher_state = match matcher_state {
            RntiMatcherState::Idle => {
                thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
                matcher_state
            }
            RntiMatcherState::StartMatching => handle_start_matching(
                &tx_gen_thread,
                &traffic_destination,
                traffic_pattern.clone(),
            ),
            RntiMatcherState::MatchingCollectDci(traffic_collection) => {
                // TODO: Use all dcis here and let this thread sleep again!
                handle_collect_dci(latest_dcis, *traffic_collection)
            }
            RntiMatcherState::MatchingProcessDci(traffic_collection) => {
                handle_process_dci(*traffic_collection,
                                   matching_log_file_path,
                                   &mut cell_rnti_ring_buffer)
            }
            RntiMatcherState::MatchingPublishRnti(rnti) => {
                tx_rnti.broadcast(rnti);
                RntiMatcherState::SleepMs(
                    MATCHING_INTERVAL_MS,
                    Box::new(RntiMatcherState::StartMatching),
                )
            }
            RntiMatcherState::MatchingError(error_type) => handle_matching_error(
                error_type,
                &tx_gen_thread,
                ),
            RntiMatcherState::SleepMs(time_ms, next_state) => {
                thread::sleep(Duration::from_millis(time_ms));
                *next_state
            }
            _ => matcher_state,
        }
    }

    Ok(())
}

fn collect_dcis(rx_dci: &mut BusReader<MessageDci>) -> Vec<MessageDci> {
    let mut dci_list = Vec::new();
    loop {
        match rx_dci.try_recv() {
            Ok(dci) => dci_list.push(dci),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => break,
        }
    }
    dci_list
}

fn handle_start_matching(
    tx_gen_thread: &SyncSender<LocalGeneratorState>,
    traffic_destination: &str,
    traffic_pattern: TrafficPattern,
) -> RntiMatcherState {
    let pattern_total_ms = traffic_pattern.total_time_ms();
    let start_timestamp_ms = chrono::Utc::now().timestamp_millis() as u64;
    let finish_timestamp_ms = start_timestamp_ms
            + (MATCHING_TRAFFIC_PATTERN_TIME_OVERLAP_FACTOR * pattern_total_ms as f64) as u64;
    let traffic_pattern_features = match TrafficPatternFeatures::from_traffic_pattern(&traffic_pattern) {
        Ok(features) => features,
        Err(_) => {
            return RntiMatcherState::MatchingError(RntiMatchingErrorType::ErrorGeneratingTrafficPatternFeatures);
        }
    };

    let traffic_collection: TrafficCollection = TrafficCollection {
        cell_traffic: Default::default(),
        start_timestamp_ms,
        finish_timestamp_ms,
        traffic_pattern_features,
    };

    let _ = tx_gen_thread.send(LocalGeneratorState::SendPattern(
        traffic_destination.to_string(),
        Box::new(traffic_pattern),
    ));
    RntiMatcherState::MatchingCollectDci(Box::new(traffic_collection))
}

fn handle_collect_dci(
    dci_list: Vec<MessageDci>,
    mut traffic_collection: TrafficCollection,
) -> RntiMatcherState {
    // TODO: Check time -> proceed to ProcessDci
    let chrono_now = chrono::Utc::now();
    let now_ms = chrono_now.timestamp_millis() as u64;
    if now_ms >= traffic_collection.finish_timestamp_ms {
        return RntiMatcherState::MatchingProcessDci(Box::new(traffic_collection));
    }

    let start_timestamp_ms_bound = traffic_collection.start_timestamp_ms * TIME_MS_TO_US_FACTOR;
    for dci in dci_list.iter() {
        if dci.ngscope_dci.time_stamp >= start_timestamp_ms_bound {
            traffic_collection.update_from_cell_dci(&dci.ngscope_dci);
        }
    }
    RntiMatcherState::MatchingCollectDci(Box::new(traffic_collection))
}

fn handle_process_dci(
    mut traffic_collection: TrafficCollection,
    log_file_path: &str,
    cell_rnti_ring_buffer: &mut CellRntiRingBuffer,
) -> RntiMatcherState {
    // Check number of packets plausability: expected ms -> expected dcis
    let mut message_rnti: MessageRnti = MessageRnti::default();

    /* log files */
    let _ = log_rnti_matching_traffic(log_file_path, &traffic_collection);

    traffic_collection.apply_basic_filter();

    let best_matches = match traffic_collection.find_best_matching_rnti() {
        Ok(matches) => matches,
        Err(e) => {
            print_info(&format!("[rntimatcher] Error during handle_process_dci: {:?}", e));
            return RntiMatcherState::MatchingError(RntiMatchingErrorType::ErrorFindingBestMatchingRnti)
        }
    };
    cell_rnti_ring_buffer.update(&best_matches);
    print_debug(&format!("DEBUG [rntimatcher] cell_rnti_ring_buffer: {:#?}", cell_rnti_ring_buffer));
    message_rnti.cell_rnti = cell_rnti_ring_buffer.most_frequent();
    RntiMatcherState::MatchingPublishRnti(message_rnti)
}

fn handle_matching_error(
    error_type: RntiMatchingErrorType,
    tx_gen_thread: &SyncSender<LocalGeneratorState>,
) -> RntiMatcherState {

    match error_type {
        RntiMatchingErrorType::ExceededDciTimestampDelta => {},
        RntiMatchingErrorType::ErrorGeneratingTrafficPatternFeatures |
        RntiMatchingErrorType::ErrorFindingBestMatchingRnti => {
            print_info(&format!(
                "[rntimatcher] error during RNTI matching: {:?}\n  -> going back to Idle",
                error_type
            ));
            let _ = tx_gen_thread.send(LocalGeneratorState::Idle);
        }
    }

    RntiMatcherState::SleepMs(
        MATCHING_INTERVAL_MS,
        Box::new(RntiMatcherState::StartMatching),
    )
}

fn finish(run_args: RunArgs) {
    let _ = run_args
        .tx_rntimatcher_state
        .send(RntiMatcherState::StoppingTrafficGeneratorThread);
    if let Some(tx_gen_thread) = run_args.tx_gen_thread_handle {
        let _ = tx_gen_thread.send(LocalGeneratorState::Stop);
    }
    if let Some(gen_thread) = run_args.gen_thread_handle {
        let _ = gen_thread.join();
    }
    let _ = send_final_state(&run_args.tx_rntimatcher_state);
}

fn deploy_traffic_generator_thread(
    rx_local_gen_state: Receiver<LocalGeneratorState>,
    local_socket_addr: String,
) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run_traffic_generator(rx_local_gen_state, local_socket_addr);
    });
    Ok(thread)
}

fn run_traffic_generator(
    rx_local_gen_state: Receiver<LocalGeneratorState>,
    local_socket_addr: String,
) -> Result<()> {
    let socket = init_udp_socket(&local_socket_addr)?;
    let mut gen_state: LocalGeneratorState = LocalGeneratorState::Idle;
    print_info(&format!(
        "[rntimatcher.gen]: \tPID {:?}",
        determine_process_id()
    ));

    loop {
        match check_rx_state(&rx_local_gen_state) {
            Ok(Some(new_state)) => gen_state = new_state,
            Ok(None) => {},
            Err(e) => {
                print_info(&format!("{}", e));
                break;
            }
        }

        match gen_state {
            LocalGeneratorState::Idle => {
                /* Sleep here, because it shall not interfere the sendpattern */
                thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
            }
            LocalGeneratorState::Stop => {
                break;
            }
            LocalGeneratorState::SendPattern(ref destination, ref mut pattern) => {
                match gen_handle_send_pattern(&socket, destination, pattern) {
                    Ok(Some(_)) => { /* stay in the state and keep sending */ },
                    Ok(None) => gen_state = LocalGeneratorState::PatternSent,
                    Err(e) => {
                        print_info(&format!("[rntimatcher.gen] Error occured while sendig the pattern: {:?}", e));
                        gen_state = LocalGeneratorState::Stop;
                    }
                }
            }
            LocalGeneratorState::PatternSent => {
                print_info("[rntimatcher.gen] Finished sending pattern!");
                gen_state = LocalGeneratorState::Idle
            }
        }
    }
    Ok(())
}

/*  --------------  */
/*      Helpers     */
/*  --------------  */

fn init_udp_socket(local_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr)?;

    Ok(socket)
}

fn check_rx_state(
    rx_local_gen_state: &Receiver<LocalGeneratorState>,
) -> Result<Option<LocalGeneratorState>> {
    match rx_local_gen_state.try_recv() {
        Ok(msg) => Ok(Some(msg)),
        Err(TryRecvError::Empty) => Ok(None),
        Err(TryRecvError::Disconnected) => Err(anyhow!(
            "[rntimatcher.local_generator] rx_local_gen_state disconnected"
        )),
    }
}

fn gen_handle_send_pattern(
    socket: &UdpSocket,
    destination: &str,
    pattern: &mut TrafficPattern,
) -> Result<Option<()>> {
    match pattern.messages.pop_front() {
        Some(msg) => {
            thread::sleep(Duration::from_millis(msg.time_ms as u64));
            socket.send_to(&msg.payload, destination)?;
            Ok(Some(()))
        }
        None => Ok(None)
    }
}

fn send_final_state(tx_rntimatcher_state: &SyncSender<RntiMatcherState>) -> Result<()> {
    Ok(tx_rntimatcher_state.send(RntiMatcherState::Stopped)?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<MainState>,
    tx_rntimtacher_state: &SyncSender<RntiMatcherState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_rntimtacher_state)?;
            Err(anyhow!("[sink] Main did not send 'Running' message"))
        }
    }
}

impl TrafficCollection {
    pub fn update_from_cell_dci(&mut self, cell_dci: &NgScopeCellDci) {
        // Ensure the cell_traffic entry exists
        let cell_id = cell_dci.cell_id as u64;
        let cell_traffic_collection = self.cell_traffic.entry(cell_id).or_default();

        // Iterate over each RNTI entry in the CellDCI
        for i in 0..cell_dci.nof_rnti as usize {
            let rnti_dci = &cell_dci.rnti_list[i];
            // Ensure the UE traffic entry exists
            let ue_traffic = cell_traffic_collection
                .traffic
                .entry(rnti_dci.rnti)
                .or_default();

            // Update the traffic for the specific TTI
            let traffic = ue_traffic.traffic.entry(cell_dci.time_stamp).or_default();
            traffic.dl_bytes += rnti_dci.dl_tbs as u64;
            traffic.ul_bytes += rnti_dci.ul_tbs as u64;
            ue_traffic.total_dl_bytes += rnti_dci.dl_tbs as u64;
            ue_traffic.total_ul_bytes += rnti_dci.ul_tbs as u64;
        }

        // Increment the nof_dci
        if cell_dci.nof_rnti == 0 {
            cell_traffic_collection.nof_empty_dci += 1;
        }
        cell_traffic_collection.nof_total_dci += 1;

        // Set timestamps
        if cell_traffic_collection.first_dci_timestamp_us == 0 {
            cell_traffic_collection.first_dci_timestamp_us = cell_dci.time_stamp;
            cell_traffic_collection.last_dci_timestamp_us = cell_dci.time_stamp;
        }
        if cell_traffic_collection.last_dci_timestamp_us < cell_dci.time_stamp {
            cell_traffic_collection.last_dci_timestamp_us = cell_dci.time_stamp;
        }
    }

    /*  Removes RNTIs by applying basic filter based on:
     *
     *   MAX TOTAL UL
     *   MIN TOTAL UL
     *   MAX UL/DCI
     *   MIN OCCURENCES
     *   MEDIAN UL
     *
     * */
    fn apply_basic_filter(&mut self) -> BasicFilterStatistics {
        let mut stats: BasicFilterStatistics = Default::default();
        let max_total_ul =
            (BASIC_FILTER_MAX_TOTAL_UL_FACTOR * self.traffic_pattern_features.total_ul_bytes as f64).round() as u64;
        let min_total_ul =
            (BASIC_FILTER_MIN_TOTAL_UL_FACTOR * self.traffic_pattern_features.total_ul_bytes as f64).round() as u64;
        let min_occurences =
            (BASIC_FILTER_MIN_OCCURENCES_FACTOR * self.traffic_pattern_features.nof_packets as f64).round() as u64;

        // Determine which RNTIs to remove
        let to_keep: HashMap<u64, HashSet<u16>> = self
            .cell_traffic
            .iter()
            .map(|(&cell_id, cell_traffic)| {
                let rntis_to_keep: HashSet<u16> = cell_traffic
                    .traffic
                    .iter()
                    /* MAX TOTAL UL */
                    .filter(|(_, ue_traffic)| {
                        if ue_traffic.total_ul_bytes > max_total_ul {
                            stats.max_total_ul += 1;
                            false
                        } else {
                            true
                        }
                    })
                    /* MIN TOTAL UL */
                    .filter(|(_, ue_traffic)| {
                        if ue_traffic.total_ul_bytes < min_total_ul {
                            stats.min_total_ul += 1;
                            false
                        } else {
                            true
                        }
                    })
                    /* MIN OCCURENCES */
                    .filter(|(_, ue_traffic)| {
                        if (ue_traffic.traffic.len() as u64) < min_occurences {
                            stats.min_occurences += 1;
                            false
                        } else {
                            true
                        }
                    })
                    /* MAX UL/DCI */
                    .filter(|(_, ue_traffic)| {
                        let mut filtered = true;
                        for (_, tx_data) in ue_traffic.traffic.iter() {
                            if tx_data.ul_bytes > BASIC_FILTER_MAX_UL_PER_DCI {
                                stats.max_ul_per_dci += 1;
                                filtered = false;
                                break;
                            }
                        }
                        filtered
                    })
                    /* ZERO MEDIAN */
                    .filter(|(_, ue_traffic)| {
                        match ue_traffic.feature_ul_bytes_median_mean_variance() {
                            Ok((median, _, _)) if median <= 0.0 => true,
                            _ => {
                                stats.zero_ul_median += 1;
                                false
                            }
                        }
                    })
                    .map(|(&rnti, _)| rnti)
                    .collect();
                (cell_id, rntis_to_keep)
            })
            .filter(|(_, rntis_to_keep)| !rntis_to_keep.is_empty())
            .collect();

        print_debug(&format!(
            "DEBUG [rntimatcher] apply basic filter: {:#?}",
            stats
        ));

        for (cell_id, rntis_to_keep) in to_keep {
            self.cell_traffic
                .get_mut(&cell_id)
                .unwrap()
                .traffic
                .retain(|key, _| rntis_to_keep.contains(key));
        }

        stats
    }

    /*
     * cell_id -> { (rnti, distance ) }
     *
     * */
    pub fn find_best_matching_rnti(&self) -> Result<HashMap<u64, u16>> {
        let pattern_std_vec = &self.traffic_pattern_features.std_vec;
        let pattern_feature_vec = &self.traffic_pattern_features.std_feature_vec;
        /* Change this to use the functional approach */
        // feature_distance_functional(&self.cell_traffic, pattern_std_vec, pattern_feature_vec);
        feature_distance_matrices(&self.cell_traffic, pattern_std_vec, pattern_feature_vec)
    }
}

impl UeTraffic {
    /*
     * Feature vector, order matters:
     *
     * DCI count (occurences)
     * Total UL bytes
     * UL bytes median
     * UL bytes mean
     * UL bytes variance
     * DCI timestamp delta median
     * DCI timestamp delta mean
     * DCI timestamp delta variance
     * */
    pub fn generate_standardized_feature_vec(&self, std_vec: &[(f64, f64)]) -> Result<Vec<f64>> {
        let mut non_std_feature_vec = vec![];
        let (ul_median, ul_mean, ul_variance) = self.feature_ul_bytes_median_mean_variance()?;
        let (tx_median, tx_mean, tx_variance) = self.feature_dci_time_delta_median_mean_variance()?;

        non_std_feature_vec.push(self.feature_dci_count());
        non_std_feature_vec.push(self.feature_total_ul_bytes());
        non_std_feature_vec.push(ul_median);
        non_std_feature_vec.push(ul_mean);
        non_std_feature_vec.push(ul_variance);
        non_std_feature_vec.push(tx_median);
        non_std_feature_vec.push(tx_mean);
        non_std_feature_vec.push(tx_variance);

        Ok(standardize_feature_vec(&non_std_feature_vec, std_vec))
    }

    pub fn feature_total_ul_bytes(&self) -> f64 {
        self.total_ul_bytes as f64
    }

    pub fn feature_dci_count(&self) -> f64 {
        self.traffic.len() as f64
    }

    pub fn feature_dci_time_delta_median_mean_variance(&self) -> Result<(f64, f64, f64)> {
        let mut sorted_timestamps: Vec<u64> = self.traffic.keys().cloned().collect();
        sorted_timestamps.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let timestamp_deltas: Vec<f64> = sorted_timestamps
            .windows(2)
            .map(|window| (window[1] - window[0]) as f64)
            .collect();

        let (mean, variance) = calculate_mean_variance(&timestamp_deltas)?;
        let median = calculate_median(&timestamp_deltas)?;

        Ok((median, mean, variance))
    }

    pub fn feature_ul_bytes_median_mean_variance(&self) -> Result<(f64, f64, f64)> {
        let ul_bytes: Vec<f64> = self
            .traffic
            .values()
            .map(|ul_dl_traffic| ul_dl_traffic.ul_bytes as f64)
            .collect();
        let (mean, variance) = calculate_mean_variance(&ul_bytes)?;
        let median = calculate_median(&ul_bytes)?;

        Ok((median, mean, variance))
    }
}

#[derive(Clone, Debug, Default)]
struct BasicFilterStatistics {
    pub max_total_ul: u64,
    pub min_total_ul: u64,
    pub max_ul_per_dci: u64,
    pub min_occurences: u64,
    pub zero_ul_median: u64,
}

fn feature_distance_functional(
    traffic: &HashMap<u64, CellTrafficCollection>,
    pattern_std_vec: &[(f64, f64)],
    pattern_feature_vec: &[f64]
) -> Result<HashMap<u64, u16>> {
    traffic.iter()
        .map(|(&cell_id, cell_traffic)| {
            let rnti_and_distance: Result<Vec<(u16, f64)>> = cell_traffic.traffic
                .iter()
                .map(|(&rnti, ue_traffic)| {
                    let std_feature_vec = ue_traffic.generate_standardized_feature_vec(pattern_std_vec)?;
                    let distance = calculate_weighted_euclidean_distance(
                            pattern_feature_vec,
                            &std_feature_vec,
                            &MATCHING_WEIGHTINGS,
                        );
                    Ok((rnti, distance))
                })
                .collect::<Result<Vec<(u16, f64)>>>();
            let mut rnti_and_distance = rnti_and_distance?;
            rnti_and_distance.sort_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap());
            Ok((cell_id, rnti_and_distance.first().unwrap().0))
        })
        .collect::<Result<HashMap<u64, u16>>>()
}

fn feature_distance_matrices(
    traffic: &HashMap<u64, CellTrafficCollection>,
    pattern_std_vec: &[(f64, f64)],
    pattern_feature_vec: &[f64]
) -> Result<HashMap<u64, u16>> {
    let num_features = pattern_std_vec.len();
    let weightings_vector = DVector::from_row_slice(&MATCHING_WEIGHTINGS);

    traffic.iter()
        .map(|(&cell_id, cell_traffic)| {
            let standardized_feature_vecs: Result<Vec<Vec<f64>>> = cell_traffic.traffic
                .values()
                .map(|ue_traffic| {
                    ue_traffic.generate_standardized_feature_vec(pattern_std_vec)
                        .map_err(|e| anyhow!(e))
                })
                .collect();

            let standardized_feature_vecs = standardized_feature_vecs?;
            let num_vectors = standardized_feature_vecs.len();

            let data: Vec<f64> = standardized_feature_vecs.into_iter().flatten().collect();
            let feature_matrix: DMatrix<f64> = DMatrix::from_row_slice(num_vectors, num_features, &data);
            
            // Uncomment and implement debug print if needed
            // print_debug(&format!("DEBUG [rntimatcher] feature_matrix: {:.2}", feature_matrix));
            
            let pattern_feature_matrix = DMatrix::from_fn(num_vectors, num_features, |_, r| pattern_feature_vec[r]);
            let euclidean_distances = calculate_weighted_euclidean_distance_matrix(
                &pattern_feature_matrix,
                &feature_matrix,
                &weightings_vector);
            
            // Uncomment and implement debug print if needed
            // print_debug(&format!("DEBUG [rntimatcher] distances: {:.2}", euclidean_distances));

            let mut rnti_and_distance: Vec<(u16, f64)> = cell_traffic.traffic.keys()
                .cloned()
                .zip(euclidean_distances.iter().cloned())
                .collect();

            rnti_and_distance.sort_by(|a, b| a.1.abs().partial_cmp(&b.1.abs()).unwrap());

            Ok((cell_id, rnti_and_distance.first().unwrap().0))
        })
        .collect::<Result<HashMap<u64, u16>>>()
}


