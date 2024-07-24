use std::os::unix::io::AsRawFd;
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
    DEFAULT_WORKER_SLEEP_US,
};
use crate::{
    logger::{log_download, log_info},
    parse::{Arguments, FlattenedDownloadArgs, Scenario},
    util::{determine_process_id, init_heap_buffer, print_debug, print_info, sockopt_get_tcp_info},
};

pub const INITIAL_SLEEP_TIME_MS: u64 = 10000;
pub const READILY_WAITING_SLEEP_TIME_MS: u64 = 500;
pub const DOWNLOADING_IDLE_SLEEP_TIME_MS: u64 = 20;
pub const RECOVERY_SLEEP_TIME_MS: u64 = 2000;

pub const TCP_STREAM_READ_BUFFER_SIZE: usize = 16384;

#[derive(Clone, Debug, PartialEq)]
pub struct DownloadStreamState {
    pub base_addr: String,
    pub path: String,
    pub rnti_share_type: u8,
    pub start_timestamp_us: u64,
    timedata: HashMap<u64, TcpLogStats>,
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
    pub timedata: HashMap<u64, TcpLogStats>,
}

#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct TcpLogStats {
    received_bytes: u64,
    rtt_us: u64,
}

pub struct DownloaderArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_downloader_state: SyncSender<DownloaderState>,
    pub tx_download_config: Bus<MessageDownloadConfig>,
}

struct RunArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
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
    let tx_downloader_state: &mut SyncSender<DownloaderState> = &mut run_args.tx_downloader_state;
    let tx_download_config: &mut Bus<MessageDownloadConfig> = &mut run_args.tx_download_config;

    tx_downloader_state.send(DownloaderState::Ready)?;
    wait_for_running(rx_app_state)?;
    print_info(&format!("[download]: \t\tPID {:?}", determine_process_id()));

    let sleep_duration = Duration::from_micros(DEFAULT_WORKER_SLEEP_US);
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
    let mut current_download: DownloadStreamState = DownloadStreamState {
        base_addr: base_addr.clone(),
        path: paths[path_list_index].clone(),
        rnti_share_type: determine_rnti_fair_share_type_by_path(&paths[path_list_index]),
        start_timestamp_us: 0,
        timedata: HashMap::new(),
    };

    loop {
        /* <precheck> */
        thread::sleep(sleep_duration);
        if check_not_stopped(rx_app_state).is_err() {
            break;
        }
        if is_idle_scenario(scenario) {
            continue; /* keep the thread running, because the Bus-reference must be kept alive for the model */
        }
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
                    start_timestamp_us: 0,
                    timedata: HashMap::new(),
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
        }
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
    DownloaderState::StartDownload
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
            download_stream_state.start_timestamp_us = chrono::Utc::now().timestamp_micros() as u64;
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
                start_timestamp_us,
                timedata,
            },
    } = params;

    match stream.read(stream_buffer) {
        Ok(chunk_size) => {
            if chunk_size == 0 {
                // End of stream
                let download_finish_timestamp_us = chrono::Utc::now().timestamp_micros() as u64;
                let total_download_bytes =
                    timedata.iter().map(|(_, v)| v.received_bytes).sum::<u64>();
                let average_rtt_us = (timedata.iter().map(|(_, v)| v.rtt_us).sum::<u64>() as f64
                    / timedata.len() as f64) as u64;
                DownloaderState::FinishDownload(DownloadFinishParameters {
                    base_addr: base_addr.to_string(),
                    path: path.to_string(),
                    start_timestamp_us: *start_timestamp_us,
                    finish_timestamp_us: download_finish_timestamp_us,
                    average_rtt_us,
                    total_download_bytes,
                    timedata: timedata.clone(),
                })
            } else {
                let now_us = chrono::Utc::now().timestamp_micros() as u64;
                match determine_socket_rtt(stream) {
                    Ok(rtt_us) => {
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
                    }
                    Err(e) => {
                        print_debug(&format!(
                            "[download] error occured while logging RTT: {:?}. Keep downloading..",
                            e
                        ));
                    }
                }
                DownloaderState::Downloading
            }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
            // No data available yet, handle as needed (e.g., sleep or continue looping)
            DownloaderState::SleepMs(
                DOWNLOADING_IDLE_SLEEP_TIME_MS,
                Box::new(DownloaderState::Downloading),
            )
        }
        Err(e) => {
            DownloaderState::ErrorStartingDownload(format!("Error starting download: {:?}", e))
        }
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
    Ok(tcp_info.tcpi_rtt.into())
}

fn determine_rnti_fair_share_type_by_path(path: &str) -> u8 {
    if path.contains("fair1") {
        1
    } else {
        0
    }
}
