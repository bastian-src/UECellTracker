use std::fs::{create_dir_all, File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, SyncSender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use arrow::array::{
    ArrayRef, ListBuilder, StructBuilder, UInt16Builder, UInt32Builder, UInt64Builder, UInt8Builder,
};
use arrow::datatypes::{DataType, Field, Fields, Schema};
use arrow::ipc::writer::FileWriter;
use arrow::record_batch::RecordBatch;

use crate::logic::downloader::DownloadFinishParameters;
use crate::logic::model_handler::LogMetric;
use crate::logic::rnti_matcher::TrafficCollection;
use crate::ngscope::types::NgScopeRntiDci;
use crate::{
    logic::{
        check_not_stopped, wait_until_running, GeneralState, MainState, WorkerState,
        DEFAULT_WORKER_SLEEP_US,
    },
    ngscope::types::NgScopeCellDci,
    parse::{Arguments, FlattenedLogArgs, DEFAULT_LOG_BASE_DIR},
    util::{determine_process_id, print_info},
};
use bus::BusReader;
use chrono::{DateTime, Local};
use once_cell::sync::Lazy;

const LOGGER_CAPACITY: usize = 1000;
const LOGGER_SENDER_POOL_SIZE: usize = 5;
const LOGGER_STOP_TIME_DELAY_MS: i64 = 5000;
const LOGGER_RELATIVE_PATH_INFO: &str = "stdout/";
const LOGGER_RELATIVE_PATH_DCI: &str = "dci/";
const LOGGER_RELATIVE_PATH_RNTI_MATCHING: &str = "rnti_matching/";
const LOGGER_RELATIVE_PATH_METRIC: &str = "metric/";
const LOGGER_RELATIVE_PATH_DOWNLOAD: &str = "download/";

#[derive(Clone, Debug, PartialEq)]
pub enum LoggerState {
    Running,
    Stopped,
    InitStopLoggingSoon,
    StopLoggingSoon,
}

impl WorkerState for LoggerState {
    fn worker_name() -> String {
        "logger".to_owned()
    }

    fn to_general_state(&self) -> GeneralState {
        match self {
            LoggerState::Running => GeneralState::Running,
            LoggerState::Stopped => GeneralState::Stopped,
            _ => GeneralState::Unknown,
        }
    }
}

pub struct LoggerArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_logger_state: SyncSender<LoggerState>,
}

struct RunArgs {
    app_args: Arguments,
    rx_app_state: BusReader<MainState>,
    tx_logger_state: SyncSender<LoggerState>,
}

#[derive(Debug)]
pub struct LogFile {
    pub path: String,
    pub last_write: DateTime<Local>,
    pub file_handle: Option<File>,
}

#[derive(Debug)]
pub struct Logger {
    pub base_dir: String,
    pub rx: Mutex<Receiver<LogMessage>>,
    pub tx_vec: Vec<Mutex<SyncSender<LogMessage>>>,
    pub stdout_file: Option<LogFile>,
    pub run_timestamp: chrono::DateTime<Local>,
}

#[derive(Debug)]
pub enum LogMessage {
    /// Log a simple string
    Info(String),
    /// NgScope cell dci
    NgScopeDci(Vec<NgScopeCellDci>),
    /// RNTI matching traffic collection
    RntiMatchingTrafficCollection(Box<TrafficCollection>),
    /// Model Metric
    Metric(Box<LogMetric>),
    /// Measurement transmission data (RTT)
    DownloadStatistics(Box<DownloadFinishParameters>),
}

/*
 * Logger thread state functions
 * */

pub fn deploy_logger(args: LoggerArgs) -> Result<JoinHandle<()>> {
    let mut run_args: RunArgs = RunArgs {
        app_args: args.app_args,
        rx_app_state: args.rx_app_state,
        tx_logger_state: args.tx_logger_state,
    };

    let builder = thread::Builder::new().name("[logger]".to_string());
    let thread = builder.spawn(move || {
        let _ = run(&mut run_args);
        finish(run_args);
    })?;
    Ok(thread)
}

fn run(run_args: &mut RunArgs) -> Result<()> {
    let rx_app_state = &mut run_args.rx_app_state;
    let tx_logger_state = &mut run_args.tx_logger_state;
    let app_args = &run_args.app_args;
    let log_args = FlattenedLogArgs::from_unflattened(app_args.clone().log.unwrap())?;
    Logger::set_base_dir(log_args.log_base_dir);

    tx_logger_state.send(LoggerState::Running)?;
    wait_for_running(rx_app_state, tx_logger_state)?;
    print_info(&format!("[logger]: \t\tPID {:?}", determine_process_id()));

    let sleep_duration = Duration::from_millis(DEFAULT_WORKER_SLEEP_US);
    let mut logger_state = LoggerState::Running;
    let mut stop_time_init_option: Option<i64> = None;

    loop {
        /* <precheck> */
        thread::sleep(sleep_duration);
        if check_not_stopped(rx_app_state).is_err() {
            logger_state = LoggerState::InitStopLoggingSoon;
        }

        match logger_state {
            LoggerState::Running => handle_running()?,
            LoggerState::Stopped => break,
            LoggerState::InitStopLoggingSoon => {
                stop_time_init_option = Some(chrono::Local::now().timestamp_millis());
                logger_state = LoggerState::StopLoggingSoon;
            }
            LoggerState::StopLoggingSoon => {
                if let Some(stop_time_init) = stop_time_init_option {
                    if chrono::Local::now().timestamp_millis() - stop_time_init
                        >= LOGGER_STOP_TIME_DELAY_MS
                    {
                        break;
                    }
                }
                handle_running()?;
            }
        }
    }

    Ok(())
}

fn finish(run_args: RunArgs) {
    let _ = send_final_state(&run_args.tx_logger_state);
}

fn handle_running() -> Result<()> {
    /* Check logger for pending message */
    loop {
        match get_logger().rx.lock().unwrap().try_recv() {
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                return Err(anyhow!(
                    "[logger] error: log Logger.rx is channel disconnected!"
                ))
            }
            Ok(log_message) => handle_log_message(log_message)?,
        }
    }
    Ok(())
}

fn handle_log_message(msg: LogMessage) -> Result<()> {
    if let LogMessage::Info(ref content) = msg {
        println!("{}", content);
    }
    let msg_type_name = msg.type_name();
    if Logger::write_log_message(msg).is_err() {
        print_info(&format!(
            "[logger] error: could not log message ({})",
            msg_type_name
        ))
    }
    Ok(())
}

fn wait_for_running(
    rx_app_state: &mut BusReader<MainState>,
    tx_logger_state: &SyncSender<LoggerState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_logger_state)?;
            Err(anyhow!("[logger] Main did not send 'Running' message"))
        }
    }
}

fn send_final_state(tx_logger_state: &SyncSender<LoggerState>) -> Result<()> {
    Ok(tx_logger_state.send(LoggerState::Stopped)?)
}

/*
 * Logger functions
 * */

pub fn log_info(info: &str) -> Result<()> {
    Logger::queue_log_message(LogMessage::Info(info.to_string()))
}

pub fn log_traffic_collection(traffic_collection: TrafficCollection) -> Result<()> {
    Logger::queue_log_message(LogMessage::RntiMatchingTrafficCollection(Box::new(
        traffic_collection,
    )))
}

pub fn log_metric(metric: LogMetric) -> Result<()> {
    Logger::queue_log_message(LogMessage::Metric(Box::new(metric)))
}

pub fn log_dci(dcis: Vec<NgScopeCellDci>) -> Result<()> {
    Logger::queue_log_message(LogMessage::NgScopeDci(dcis))
}

pub fn log_download(download: DownloadFinishParameters) -> Result<()> {
    Logger::queue_log_message(LogMessage::DownloadStatistics(Box::new(download)))
}

#[allow(unknown_lints)]
pub fn get_logger() -> &'static mut Lazy<Logger> {
    static mut GLOBAL_LOGGER: Lazy<Logger> = Lazy::new(|| {
        let (tx, rx) = sync_channel::<LogMessage>(LOGGER_CAPACITY);
        let tx_vec: Vec<Mutex<SyncSender<LogMessage>>> = (0..LOGGER_SENDER_POOL_SIZE)
            .map(|_| Mutex::new(tx.clone()))
            .collect();

        let run_timestamp = chrono::Local::now();
        let run_timestamp_formatted = run_timestamp.format("%Y_%m_%d-%H_%M_%S").to_string();
        let base_dir = format!("{}run-{}/", DEFAULT_LOG_BASE_DIR, run_timestamp_formatted);

        Logger {
            base_dir,
            rx: Mutex::new(rx),
            tx_vec,
            stdout_file: None,
            run_timestamp,
        }
    });
    #[allow(static_mut_refs)]
    unsafe {
        &mut GLOBAL_LOGGER
    }
}

impl Logger {
    /* Find an unused Sender (use last if none is lockable) */
    fn lockable_sender(&self) -> &Mutex<SyncSender<LogMessage>> {
        for sender in self.tx_vec.iter().take(self.tx_vec.len() - 1) {
            if sender.try_lock().is_ok() {
                return sender;
            }
        }
        return self.tx_vec.last().unwrap();
    }

    pub fn set_base_dir(new_base_dir: String) {
        let run_timestamp_formatted = get_logger().run_timestamp.format("%Y_%m_%d-%H_%M_%S").to_string();
        get_logger().base_dir = format!("{}run-{}/", new_base_dir, run_timestamp_formatted);
    }

    pub fn queue_log_message(msg: LogMessage) -> Result<()> {
        let logger = get_logger();
        let sender: &Mutex<SyncSender<LogMessage>> = logger.lockable_sender();
        sender.lock().unwrap().send(msg)?;
        Ok(())
    }

    pub fn write_log_message(msg: LogMessage) -> Result<()> {
        let logger: &mut Logger = get_logger();

        /* check open file handle for stdout */
        if let LogMessage::Info(_) = msg {
            let stdout_file_handle = {
                let stdout_file = logger.stdout_file.get_or_insert_with(|| {
                    let stdout_file_path = msg.file_path(&logger.base_dir, &logger.run_timestamp);
                    LogFile {
                        path: stdout_file_path,
                        last_write: Local::now(),
                        file_handle: None,
                    }
                });

                if let Some(parent) = Path::new(&stdout_file.path).parent() {
                    create_dir_all(parent)?;
                }
                stdout_file.file_handle.get_or_insert_with(|| {
                    OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&stdout_file.path)
                        .expect("Failed to open log file")
                })
            };
            msg.write_to_file(stdout_file_handle)?;
            logger.stdout_file.as_mut().unwrap().last_write = chrono::Local::now();
            return Ok(());
        }

        /* open new file handle for others */
        let file_path = msg.file_path(&logger.base_dir, &logger.run_timestamp);
        if let Some(parent) = Path::new(&file_path).parent() {
            create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(file_path)
            .expect("Failed to open log file");
        msg.write_to_file(&mut file)?;

        Ok(())
    }
}

impl LogMessage {
    pub fn type_name(&self) -> String {
        match self {
            LogMessage::Info(_) => "info",
            LogMessage::NgScopeDci(_) => "ngscope dci",
            LogMessage::RntiMatchingTrafficCollection(_) => "rnti traffic collection",
            LogMessage::Metric(_) => "metric",
            LogMessage::DownloadStatistics(_) => "download",
        }
        .to_string()
    }

    pub fn file_path(&self, base_dir: &str, run_timestamp: &DateTime<Local>) -> String {
        let run_timestamp_formatted = run_timestamp.format("%Y_%m_%d-%H_%M_%S").to_string();
        let now_formatted = Local::now().format("%d-%H_%M_%S%.3f").to_string();

        let message_type_file_path: String = match self {
            LogMessage::Info(_) => {
                format!(
                    "{}run_{}.log",
                    LOGGER_RELATIVE_PATH_INFO, run_timestamp_formatted
                )
            }
            LogMessage::NgScopeDci(_) => {
                format!(
                    "{}run_{}_cell_data_{}.arrow",
                    LOGGER_RELATIVE_PATH_DCI, run_timestamp_formatted, now_formatted
                )
            }
            LogMessage::Metric(_) => {
                format!(
                    "{}run_{}_metric.jsonl",
                    LOGGER_RELATIVE_PATH_METRIC, run_timestamp_formatted
                )
            }
            LogMessage::RntiMatchingTrafficCollection(_) => {
                format!(
                    "{}run_{}_traffic_collection.jsonl",
                    LOGGER_RELATIVE_PATH_RNTI_MATCHING, run_timestamp_formatted
                )
            }
            LogMessage::DownloadStatistics(finish_parameters) => {
                format!(
                    "{}run_{}_download_{}.jsonl",
                    LOGGER_RELATIVE_PATH_DOWNLOAD, run_timestamp_formatted,
                    finish_parameters.path.replace('/', "_")
                )
            }
        };
        format!("{}{}", base_dir, message_type_file_path)
    }

    pub fn write_to_file(&self, file: &mut File) -> Result<()> {
        match self {
            LogMessage::Info(info_msg) => {
                writeln!(file, "{}", info_msg)?;
            }
            LogMessage::RntiMatchingTrafficCollection(traffic_collection) => {
                let json_string = serde_json::to_string(traffic_collection)?;
                writeln!(file, "{}", json_string)?;
            }
            LogMessage::NgScopeDci(ngscope_dci_list) => {
                write_arrow_ipc(create_schema(), ngscope_dci_list.to_vec(), file)?
            }
            LogMessage::Metric(metric) => {
                let json_string = serde_json::to_string(metric)?;
                writeln!(file, "{}", json_string)?;
            }
            LogMessage::DownloadStatistics(download) => {
                let json_string = serde_json::to_string(download)?;
                writeln!(file, "{}", json_string)?;
            }
        }
        file.flush()?;
        Ok(())
    }
}

/*
 * Helpers for writing Vec<NgScopeCellDci> as Apache Arrow to disk
 * */

fn create_rnti_fields() -> Fields {
    Fields::from(vec![
        Field::new("rnti", DataType::UInt16, true),
        Field::new("dl_tbs_bit", DataType::UInt32, true),
        Field::new("dl_prb", DataType::UInt8, true),
        Field::new("dl_no_tbs_prb", DataType::UInt8, true),
        Field::new("ul_tbs_bit", DataType::UInt32, true),
        Field::new("ul_prb", DataType::UInt8, true),
        Field::new("ul_no_tbs_prb", DataType::UInt8, true),
    ])
}

fn create_schema() -> Arc<Schema> {
    let rnti_struct = DataType::Struct(create_rnti_fields());

    Arc::new(Schema::new(vec![
        Field::new("timestamp", DataType::UInt64, false),
        Field::new("nof_rnti", DataType::UInt8, false),
        Field::new(
            "rnti_list",
            DataType::List(Arc::new(Field::new("item", rnti_struct, true))),
            true,
        ),
    ]))
}

fn write_arrow_ipc(schema: Arc<Schema>, data: Vec<NgScopeCellDci>, file: &File) -> Result<()> {
    let mut timestamp_builder = UInt64Builder::with_capacity(data.len());
    let mut nof_rntis_builder = UInt8Builder::with_capacity(data.len());
    let mut rnti_list_builder =
        ListBuilder::new(StructBuilder::from_fields(create_rnti_fields(), data.len()));

    for cell_dci in &data {
        timestamp_builder.append_value(cell_dci.time_stamp);
        nof_rntis_builder.append_value(cell_dci.nof_rnti);

        if cell_dci.nof_rnti == 0 {
            rnti_list_builder.append(false); // Append null for an empty list
        } else {
            let rnti_struct_builder = rnti_list_builder.values();
            append_rnti_list_to_struct(
                rnti_struct_builder,
                &cell_dci.rnti_list[0..cell_dci.nof_rnti as usize],
            );
            rnti_list_builder.append(true);
        }
    }

    let timestamp_array = Arc::new(timestamp_builder.finish()) as ArrayRef;
    let nof_rntis_array = Arc::new(nof_rntis_builder.finish()) as ArrayRef;
    let rnti_list_array = Arc::new(rnti_list_builder.finish()) as ArrayRef;

    let batch = RecordBatch::try_new(
        schema,
        vec![timestamp_array, nof_rntis_array, rnti_list_array],
    )?;

    let mut writer = FileWriter::try_new(file, &batch.schema())?;
    writer.write(&batch)?;
    writer.finish()?;

    Ok(())
}

fn append_rnti_list_to_struct(
    rnti_struct_builder: &mut StructBuilder,
    rnti_list: &[NgScopeRntiDci],
) {
    for rnti_dci in rnti_list.iter() {
        rnti_struct_builder
            .field_builder::<UInt16Builder>(0)
            .unwrap()
            .append_value(rnti_dci.rnti);

        rnti_struct_builder
            .field_builder::<UInt32Builder>(1)
            .unwrap()
            .append_value(rnti_dci.dl_tbs_bit);
        rnti_struct_builder
            .field_builder::<UInt8Builder>(2)
            .unwrap()
            .append_value(rnti_dci.dl_prb);
        rnti_struct_builder
            .field_builder::<UInt8Builder>(3)
            .unwrap()
            .append_value(rnti_dci.dl_no_tbs_prb);

        rnti_struct_builder
            .field_builder::<UInt32Builder>(4)
            .unwrap()
            .append_value(rnti_dci.ul_tbs_bit);
        rnti_struct_builder
            .field_builder::<UInt8Builder>(5)
            .unwrap()
            .append_value(rnti_dci.ul_prb);
        rnti_struct_builder
            .field_builder::<UInt8Builder>(6)
            .unwrap()
            .append_value(rnti_dci.ul_no_tbs_prb);

        rnti_struct_builder.append(true);
    }
}
