use std::os::unix::io::AsRawFd;
use std::sync::mpsc::TryRecvError;
use std::{
    collections::HashMap,
    io::{self, Read, Write},
    net::{Shutdown, TcpStream},
    sync::mpsc::SyncSender,
    thread::{self, JoinHandle},
    time::Duration,
};

use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};
use serde_derive::{Deserialize, Serialize};

use super::{
    check_not_stopped, wait_until_running, DownloaderState, MainState, MessageDownloadConfig,
    MessageDci, MessageRnti, DEFAULT_WORKER_SLEEP_MS,
};
use crate::ngscope::types::NgScopeCellDci;
use crate::{
    logger::{log_download, log_info},
    parse::{Arguments, FlattenedDownloadArgs, Scenario},
    util::{determine_process_id, init_heap_buffer, print_debug, print_info, sockopt_get_tcp_info},
};

pub const INITIAL_SLEEP_TIME_MS: u64 = 20_000;
pub const READILY_WAITING_SLEEP_TIME_MS: u64 = 500;
pub const DOWNLOADING_IDLE_SLEEP_TIME_MS: u64 = 20;
pub const RECOVERY_SLEEP_TIME_MS: u64 = 2_000;
pub const BETWEEN_DOWNLOADS_SLEEP_TIME_MS: u64 = 1_000;
pub const RESTART_TIMEOUT_US: u64 = 2_000_000;
pub const POST_DOWNLOAD_TIME_US: u64 = 2_000_000;

pub const TCP_STREAM_READ_BUFFER_SIZE: usize = 100_000;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct DownloadStreamState {
    pub base_addr: String,
    pub path: String,
    pub rnti_share_type: u8,
    pub last_rtt_us: Option<u64>,
    pub start_timestamp_us: u64,
    pub finish_timestamp_us: Option<u64>,
    pub timedata: HashMap<u64, TcpLogStats>,
    pub dci_total_dl_bit: u64,
    pub dci_rnti_dl_bit: u64,
    pub dci_total_dl_prb_with_tbs: u64,
    pub dci_total_dl_prb_no_tbs: u64,
    pub dci_rnti_dl_prb_with_tbs: u64,
    pub dci_rnti_dl_prb_no_tbs: u64,
}

#[derive(Debug)]
pub struct DownloadingParameters<'a> {
    pub stream: &'a mut TcpStream,
    pub stream_buffer: &'a mut Box<[u8]>,
    pub tx_download_config: &'a mut Bus<MessageDownloadConfig>,
    pub download_stream_state: &'a mut DownloadStreamState,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct DownloadFinishParameters {
    pub base_addr: String,
    pub path: String,
    pub start_timestamp_us: u64,
    pub finish_timestamp_us: u64,
    pub average_rtt_us: u64,
    pub total_download_bytes: u64,
    pub dci_total_dl_bit: u64,
    pub dci_rnti_dl_bit: u64,
    pub dci_total_dl_prb_with_tbs: u64,
    pub dci_total_dl_prb_no_tbs: u64,
    pub dci_rnti_dl_prb_with_tbs: u64,
    pub dci_rnti_dl_prb_no_tbs: u64,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TcpLogStats {
    received_bytes: u64,
    rtt_us: u64,
}

pub struct DownloaderArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub tx_downloader_state: SyncSender<DownloaderState>,
    pub tx_download_config: Bus<MessageDownloadConfig>,
}

struct RunArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub tx_downloader_state: SyncSender<DownloaderState>,
    pub tx_download_config: Bus<MessageDownloadConfig>,
    pub stream_handle: Option<TcpStream>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DownloadConfig {
    pub rtt_us: u64,
    pub rnti_share_type: u8,
}

pub fn deploy_downloader(args: DownloaderArgs) -> Result<JoinHandle<()>> {
    let mut run_args = RunArgs {
        app_args: args.app_args,
        rx_app_state: args.rx_app_state,
        rx_dci: args.rx_dci,
        rx_rnti: args.rx_rnti,
        tx_downloader_state: args.tx_downloader_state,
        tx_download_config: args.tx_download_config,
        stream_handle: None,
    };

    let builder = thread::Builder::new().name("[download]".to_string());
    let thread = builder.spawn(move || {
        let _ = run(&mut run_args);
        finish(run_args);
    })?;
    Ok(thread)
}

fn send_final_state(tx_download_state: &SyncSender<DownloaderState>) -> Result<()> {
    Ok(tx_download_state.send(DownloaderState::Stopped)?)
}

fn is_idle_scenario(scenario: Scenario) -> bool {
    match scenario {
        Scenario::TrackCellDciOnly => true,
        Scenario::TrackUeAndEstimateTransportCapacity => true,
        Scenario::PerformMeasurement => false,
    }
}

fn wait_for_running(rx_app_state: &mut BusReader<MainState>) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => Err(anyhow!("[download] Main did not send 'Running' message")),
    }
}

fn run(run_args: &mut RunArgs) -> Result<()> {
    let app_args = &run_args.app_args;
    let rx_app_state: &mut BusReader<MainState> = &mut run_args.rx_app_state;
    let rx_dci: &mut BusReader<MessageDci> = &mut run_args.rx_dci;
    let rx_rnti: &mut BusReader<MessageRnti> = &mut run_args.rx_rnti;
    let tx_downloader_state: &mut SyncSender<DownloaderState> = &mut run_args.tx_downloader_state;
    let tx_download_config: &mut Bus<MessageDownloadConfig> = &mut run_args.tx_download_config;

    tx_downloader_state.send(DownloaderState::Ready)?;
    wait_for_running(rx_app_state)?;
    print_info(&format!("[download]: \t\tPID {:?}", determine_process_id()));

    let download_args =
        FlattenedDownloadArgs::from_unflattened(app_args.clone().download.unwrap())?;
    let scenario = app_args.scenario.unwrap();
    let stream_handle = &mut run_args.stream_handle;
    let mut stream_buffer = init_heap_buffer(TCP_STREAM_READ_BUFFER_SIZE);
    let base_addr = download_args.download_base_addr;
    let mut path_list_index = 0;
    let paths = download_args.download_paths;

    let mut downloader_state = DownloaderState::SleepMs(
        INITIAL_SLEEP_TIME_MS,
        Box::new(DownloaderState::StartDownload),
    );
    let mut current_rnti: Option<u16> = None;
    let mut current_download: DownloadStreamState = DownloadStreamState {
        base_addr: base_addr.clone(),
        path: paths[path_list_index].clone(),
        rnti_share_type: determine_rnti_fair_share_type_by_path(&paths[path_list_index]),
        ..Default::default()
    };

    loop {
        /* <precheck> */
        if check_not_stopped(rx_app_state).is_err() {
            break;
        }
        unpack_all_rnti_messages(rx_rnti, &mut current_rnti)?;
        unpack_all_dci_messages(rx_dci, &mut current_download, &downloader_state, current_rnti)?;
        if is_idle_scenario(scenario) {
            continue; /* keep the thread running, because the Bus-reference must be kept alive for the model */
        }
        thread::sleep(Duration::from_millis(DEFAULT_WORKER_SLEEP_MS));
        /* </precheck> */

        downloader_state = match downloader_state {
            DownloaderState::Stopped => break,
            DownloaderState::Ready => DownloaderState::SleepMs(
                READILY_WAITING_SLEEP_TIME_MS,
                Box::new(DownloaderState::Ready),
            ),
            DownloaderState::SleepMs(sleep_time_ms, next_state) => {
                thread::sleep(Duration::from_millis(sleep_time_ms));
                *next_state
            }
            DownloaderState::StartDownload => {
                let download_path = paths[path_list_index].clone();
                current_download = DownloadStreamState {
                    base_addr: base_addr.clone(),
                    path: download_path.clone(),
                    rnti_share_type: determine_rnti_fair_share_type_by_path(&download_path),
                    ..Default::default()
                };
                handle_start_download(&mut current_download, stream_handle)
            }
            DownloaderState::Downloading => {
                let params = DownloadingParameters {
                    stream: stream_handle.as_mut().unwrap(),
                    stream_buffer: &mut stream_buffer,
                    tx_download_config,
                    download_stream_state: &mut current_download,
                };
                handle_downloading(params)
            }
            DownloaderState::FinishDownload(params) => {
                path_list_index = (path_list_index + 1) % paths.len();
                *stream_handle = None;
                handle_finish_download(params)
            }
            DownloaderState::ErrorStartingDownload(message) => {
                print_info(&format!("[download] error during download: {}", message));
                DownloaderState::SleepMs(
                    RECOVERY_SLEEP_TIME_MS,
                    Box::new(DownloaderState::StartDownload),
                )
            }
            DownloaderState::PostDownload => {
                handle_post_download(&mut current_download)
            }
        }
    }

    Ok(())
}

fn unpack_all_rnti_messages(
    rx_rnti: &mut BusReader<MessageRnti>,
    rnti_option: &mut Option<u16>
) -> Result<()> {
    let mut last_rnti_option: Option<u16> = *rnti_option;
    loop {
        match rx_rnti.try_recv() {
            Ok(rnti_msg) => {
                if let Some(rnti) = rnti_msg.cell_rnti.values().copied().next() {
                    print_debug(&format!("DEBUG [download] new RNTI: {}", rnti));
                    last_rnti_option = Some(rnti);
                }
            }
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => return Err(anyhow!("[download] error: rx_rnti disconnected"))
        }
    }
    *rnti_option = last_rnti_option;
    Ok(())
}

fn unpack_all_dci_messages(
    rx_dci: &mut BusReader<MessageDci>,
    download_stream_state: &mut DownloadStreamState,
    downloader_state: &DownloaderState,
    rnti_option: Option<u16>
) -> Result<()> {
    while let Ok(dci) = rx_dci.try_recv() {
        if let DownloaderState::Downloading | DownloaderState::PostDownload = downloader_state {
            if let MessageDci::CellDci(ngscope_dci) = dci {
                if ngscope_dci.time_stamp >= download_stream_state.start_timestamp_us {
                    if let Some(finish_timestamp_us) = download_stream_state.finish_timestamp_us {
                        if ngscope_dci.time_stamp > finish_timestamp_us {
                            continue;
                        } else {
                            print_debug("DEBUG [download] Collecting DCI in PostDownload state!!!");
                        }
                    }
                    download_stream_state.add_ngscope_dci(*ngscope_dci, rnti_option);
                }
            }
        }
    }

    if let Err(TryRecvError::Disconnected) = rx_dci.try_recv() {
        return Err(anyhow!("[download] error: rx_dci disconnected"));
    }

    Ok(())
}

fn finish(run_args: RunArgs) {
    if let Some(stream) = run_args.stream_handle {
        let _ = stream.shutdown(Shutdown::Both);
    }
    let _ = send_final_state(&run_args.tx_downloader_state);
}

fn handle_finish_download(finish_parameters: DownloadFinishParameters) -> DownloaderState {
    if let Err(e) = log_download(finish_parameters) {
        let _ = log_info(&format!(
            "[download] error occured while logging download statistics: {:?}",
            e
        ));
    }
    DownloaderState::SleepMs(BETWEEN_DOWNLOADS_SLEEP_TIME_MS, Box::new(DownloaderState::StartDownload))
}

fn handle_start_download(
    download_stream_state: &mut DownloadStreamState,
    stream_option: &mut Option<TcpStream>,
) -> DownloaderState {
    match create_download_stream(
        &download_stream_state.base_addr,
        &download_stream_state.path,
    ) {
        Ok(stream) => {
            download_stream_state.start_timestamp_us = chrono::Local::now().timestamp_micros() as u64;
            *stream_option = Some(stream);
            DownloaderState::Downloading
        }
        Err(e) => {
            DownloaderState::ErrorStartingDownload(format!("Error starting download: {:?}", e))
        }
    }
}

fn handle_downloading(params: DownloadingParameters) -> DownloaderState {
    let DownloadingParameters {
        stream,
        stream_buffer,
        tx_download_config,
        download_stream_state:
            DownloadStreamState {
                base_addr,
                path,
                rnti_share_type,
                last_rtt_us,
                start_timestamp_us,
                finish_timestamp_us,
                timedata,
                dci_total_dl_bit,
                dci_rnti_dl_bit,
                dci_total_dl_prb_with_tbs,
                dci_total_dl_prb_no_tbs,
                dci_rnti_dl_prb_with_tbs,
                dci_rnti_dl_prb_no_tbs,
            },
    } = params;

    match stream.read(stream_buffer) {
        Ok(chunk_size) => {
            if chunk_size == 0 {
                // End of stream
                *finish_timestamp_us = Some(chrono::Local::now().timestamp_micros() as u64);
                DownloaderState::PostDownload

            } else {
                let now_us = chrono::Local::now().timestamp_micros() as u64;
                if let Some(rtt_us) = try_to_decode_rtt(&stream_buffer[0..chunk_size], last_rtt_us) {
                    timedata.entry(now_us).or_insert(TcpLogStats {
                        received_bytes: chunk_size as u64,
                        rtt_us,
                    });
                    tx_download_config.broadcast(MessageDownloadConfig {
                        config: DownloadConfig {
                            rtt_us,
                            rnti_share_type: *rnti_share_type,
                        },
                    });
                } else {
                    print_debug("[download] error occured while logging RTT: \
                    Cannot decode RTT and no last_rtt given. Keep downloading..");
                }
                DownloaderState::Downloading
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            // If no packets have arrived in a certain time, restart the download
            if timedata.is_empty() {
                let now_us = chrono::Local::now().timestamp_micros() as u64;
                if (now_us - *start_timestamp_us) > RESTART_TIMEOUT_US {
                    return DownloaderState::ErrorStartingDownload("Downloading Timeout!".to_string())
                }
            }
            DownloaderState::Downloading
        }
        Err(ref e) if e.kind() == io::ErrorKind::ConnectionReset => {
            // Early reset, handle as server closed the connection
            let download_finish_timestamp_us = chrono::Local::now().timestamp_micros() as u64;
            let total_download_bytes = determine_average_download_bytes(timedata);
            let average_rtt_us = determine_average_rtt_us(timedata);
            DownloaderState::FinishDownload(DownloadFinishParameters {
                base_addr: base_addr.to_string(),
                path: path.to_string(),
                start_timestamp_us: *start_timestamp_us,
                finish_timestamp_us: download_finish_timestamp_us,
                average_rtt_us,
                total_download_bytes,
                dci_total_dl_bit: *dci_total_dl_bit,
                dci_total_dl_prb_with_tbs: *dci_total_dl_prb_with_tbs,
                dci_total_dl_prb_no_tbs: *dci_total_dl_prb_no_tbs,
                dci_rnti_dl_bit: *dci_rnti_dl_bit,
                dci_rnti_dl_prb_with_tbs: *dci_rnti_dl_prb_with_tbs,
                dci_rnti_dl_prb_no_tbs: *dci_rnti_dl_prb_no_tbs,
            })
        }
        Err(e) => {
            DownloaderState::ErrorStartingDownload(format!("Error during download: {:?}", e))
        }
    }
}

fn handle_post_download(download_stream_state: &mut DownloadStreamState) -> DownloaderState {
    let DownloadStreamState {
        base_addr,
        path,
        start_timestamp_us,
        finish_timestamp_us,
        timedata,
        dci_total_dl_bit,
        dci_rnti_dl_bit,
        dci_total_dl_prb_with_tbs,
        dci_total_dl_prb_no_tbs,
        dci_rnti_dl_prb_with_tbs,
        dci_rnti_dl_prb_no_tbs,
        ..
    } = download_stream_state;

    let now_us = chrono::Local::now().timestamp_micros() as u64;
    if now_us < (finish_timestamp_us.unwrap() + POST_DOWNLOAD_TIME_US) {
        // Stay in PostDownload and keep collecting DCI
        DownloaderState::PostDownload
    } else {
        let total_download_bytes = determine_average_download_bytes(timedata);
        let average_rtt_us = determine_average_rtt_us(timedata);
        DownloaderState::FinishDownload(DownloadFinishParameters {
            base_addr: base_addr.to_string(),
            path: path.to_string(),
            start_timestamp_us: *start_timestamp_us,
            finish_timestamp_us: finish_timestamp_us.unwrap(),
            average_rtt_us,
            total_download_bytes,
            dci_total_dl_bit: *dci_total_dl_bit,
            dci_total_dl_prb_with_tbs: *dci_total_dl_prb_with_tbs,
            dci_total_dl_prb_no_tbs: *dci_total_dl_prb_no_tbs,
            dci_rnti_dl_bit: *dci_rnti_dl_bit,
            dci_rnti_dl_prb_with_tbs: *dci_rnti_dl_prb_with_tbs,
            dci_rnti_dl_prb_no_tbs: *dci_rnti_dl_prb_no_tbs,
        })
    }

}

fn create_download_stream(base_addr: &str, path: &str) -> Result<TcpStream> {
    let mut stream = TcpStream::connect(base_addr)?;

    print_debug(&format!(
        "DEBUG [download] create_download_stream.path: {}",
        path
    ));

    // Send HTTP GET request
    let request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: example.com\r\n\
         Connection: close\r\n\r\n",
        path
    );
    stream.write_all(request.as_bytes())?;
    stream.set_nonblocking(true)?;

    Ok(stream)
}

fn determine_socket_rtt(stream: &mut TcpStream) -> Result<u64> {
    let socket_file_descriptor: i32 = stream.as_raw_fd();
    let tcp_info = sockopt_get_tcp_info(socket_file_descriptor)?;
    let rtt_us = tcp_info.tcpi_rtt as u64;
    print_debug(&format!("DEBUG [determine_socket_rtt] rtt: {:?}", rtt_us));
    Ok(rtt_us)
}

fn try_to_decode_rtt(buffer: &[u8], last_rtt_us: &mut Option<u64>) -> Option<u64> {
    // Only search in a small portion of the whole buffer
    let partial_buffer = if buffer.len() > 40 {
        &buffer[buffer.len() - 40..]
    } else {
        buffer
    };

    if let Some(rtt_us) = try_to_extract_last_rtt(partial_buffer) {
        *last_rtt_us = Some(rtt_us);
        Some(rtt_us)
    } else {
        *last_rtt_us
    }
}

fn try_to_extract_last_rtt(buffer: &[u8]) -> Option<u64> {
    let marker_start = [0xAA, 0xAB, 0xAC];
    let marker_end = [0xBA, 0xBB, 0xBC];

    if buffer.len() < 10 {
        return None
    }

    let mut possible_start_index: usize = buffer.len() - 10 - 1;
    let mut possible_end_index = possible_start_index + 7;
    loop {
        if buffer[possible_start_index..].starts_with(&marker_start) &&
            buffer[possible_end_index..].starts_with(&marker_end) {
            let rtt = ((buffer[possible_start_index + 3] as u32) << 24)
                    | ((buffer[possible_start_index + 4] as u32) << 16)
                    | ((buffer[possible_start_index + 5] as u32) << 8)
                    | ( buffer[possible_start_index + 6] as u32);
            return Some(rtt as u64);
        }
        if possible_start_index > 0 {
            possible_start_index -= 1;
            possible_end_index = possible_start_index + 7;
        } else {
            return None;
        }
    }
}

fn determine_rnti_fair_share_type_by_path(path: &str) -> u8 {
    if path.contains("fair2") {
        2
    } else if path.contains("fair1") {
        1
    } else {
        0
    }
}

impl DownloadStreamState {
    fn add_ngscope_dci(&mut self, ngscope_dci: NgScopeCellDci, rnti_option: Option<u16>) {
        if let Some(rnti) = rnti_option {
            self.dci_rnti_dl_bit += ngscope_dci.rnti_list
                .iter()
                .take(ngscope_dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.rnti == rnti)
                .map(|rnti_dci| rnti_dci.dl_tbs_bit as u64)
                .sum::<u64>();
            self.dci_rnti_dl_prb_with_tbs += ngscope_dci.rnti_list
                .iter()
                .take(ngscope_dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.rnti == rnti)
                .map(|rnti_dci| rnti_dci.dl_prb as u64)
                .sum::<u64>();
            self.dci_rnti_dl_prb_no_tbs += ngscope_dci.rnti_list
                .iter()
                .take(ngscope_dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.rnti == rnti)
                .map(|rnti_dci| rnti_dci.dl_no_tbs_prb as u64)
                .sum::<u64>();
        }
        self.dci_total_dl_bit += ngscope_dci.rnti_list
            .iter()
            .take(ngscope_dci.nof_rnti as usize)
            .map(|rnti_dci| rnti_dci.dl_tbs_bit as u64)
            .sum::<u64>();
        self.dci_total_dl_prb_with_tbs += ngscope_dci.rnti_list
            .iter()
            .take(ngscope_dci.nof_rnti as usize)
            .map(|rnti_dci| rnti_dci.dl_prb as u64)
            .sum::<u64>();
        self.dci_total_dl_prb_no_tbs += ngscope_dci.rnti_list
            .iter()
            .take(ngscope_dci.nof_rnti as usize)
            .map(|rnti_dci| rnti_dci.dl_no_tbs_prb as u64)
            .sum::<u64>();
    }
}

fn determine_average_download_bytes(timedata: &HashMap<u64, TcpLogStats>) -> u64 {
    timedata.iter().map(|(_, v)| v.received_bytes).sum::<u64>()
}

fn determine_average_rtt_us(timedata: &HashMap<u64, TcpLogStats>) -> u64 {
    (timedata.iter().map(|(_, v)| v.rtt_us).sum::<u64>() as f64
     / timedata.len() as f64) as u64
}
