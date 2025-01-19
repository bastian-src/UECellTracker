/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use anyhow::{anyhow, Result};
use clap::{Args, Command, CommandFactory, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

use crate::{
    logic::traffic_patterns::RntiMatchingTrafficPatternType,
    util::{print_info,UpdateIfSome}
};

pub const DEFAULT_SCENARIO: Scenario = Scenario::TrackUeAndEstimateTransportCapacity;
pub const DEFAULT_VERBOSE: bool = true;
pub const DEFAULT_CELL_API: CellApiConfig = CellApiConfig::Milesight;

pub const DEFAULT_MILESIGHT_ADDRESS: &str = "http://127.0.0.1:8080";
pub const DEFAULT_MILESIGHT_USER: &str = "root";
pub const DEFAULT_MILESIGHT_AUTH: &str = "root-password";

//port is implicitly always 7573 or something like that; might make sense to make it modifiable..
pub const DEFAULT_DEVPUB_ADDRESS: &str = "127.0.0.1";
pub const DEFAULT_DEVPUB_AUTH: &str = "some_auth";

pub const DEFAULT_NG_PATH: &str = "/dev_ws/dependencies/ng-scope/build_x86/ngscope/src/ngscope";
pub const DEFAULT_NG_LOCAL_ADDR: &str = "0.0.0.0:9191";
pub const DEFAULT_NG_SERVER_ADDR: &str = "0.0.0.0:6767";
pub const DEFAULT_NG_LOG_FILE: &str = "./.ng_scope_log.txt";
pub const DEFAULT_NG_START_PROCESS: bool = true;
pub const DEFAULT_NG_LOG_DCI: bool = true;
pub const DEFAULT_NG_LOG_DCI_BATCH_SIZE: u64 = 60000;
pub const DEFAULT_NG_SDR_A_SERIAL: &str = "3295B62";
pub const DEFAULT_NG_SDR_A_N_ID: i16 = -1;

pub const DEFAULT_MATCHING_LOCAL_ADDR: &str = "0.0.0.0:9292";
pub const DEFAULT_MATCHING_TRAFFIC_PATTERN: &[RntiMatchingTrafficPatternType] = &[RntiMatchingTrafficPatternType::A];
pub const DEFAULT_MATCHING_TRAFFIC_DEST: &str = "127.0.0.1:9494";
pub const DEFAULT_MATCHING_LOG_TRAFFIC: bool = true;

pub const DEFAULT_MODEL_INTERVAL_VALUE: f64 = 1.0;
pub const DEFAULT_MODEL_INTERVAL_TYPE: DynamicValue = DynamicValue::RttFactor;
pub const DEFAULT_MODEL_SMOOTHING_VALUE: f64 = 1.0;
pub const DEFAULT_MODEL_SMOOTHING_TYPE: DynamicValue = DynamicValue::RttFactor;
pub const DEFAULT_MODEL_LOG_METRIC: bool = true;

pub const DEFAULT_LOG_BASE_DIR: &str = "./.logs.ue/";
pub const DEFAULT_DOWNLOAD_BASE_ADDR: &str = "127.0.0.1:9393";
pub const DEFAULT_DOWNLOAD_PATHS: &[&str] = &[
    "/10s/cubic",
    "/10s/bbr",
    "/10s/reno",
    "/10s/l2b/fair0/init",
    "/10s/l2b/fair0/upper",
    "/10s/l2b/fair0/init_and_upper",
    "/10s/l2b/fair0/direct",
    "/10s/l2b/fair1/init",
    "/10s/l2b/fair1/upper",
    "/10s/l2b/fair1/init_and_upper",
    "/10s/l2b/fair1/direct",
    "/60s/cubic",
    "/60s/bbr",
    "/60s/reno",
    "/60s/l2b/fair0/init",
    "/60s/l2b/fair0/upper",
    "/60s/l2b/fair0/init_and_upper",
    "/60s/l2b/fair0/direct",
    "/60s/l2b/fair1/init",
    "/60s/l2b/fair1/upper",
    "/60s/l2b/fair1/init_and_upper",
    "/60s/l2b/fair1/direct",
];

#[derive(Debug, Clone, PartialEq, Parser, Serialize, Deserialize)]
#[command(author, version, about, long_about = None, next_line_help = true)]
#[command(propagate_version = true)]
pub struct Arguments {
    /// The scenario to run
    #[arg(long, value_enum, required = false)]
    pub scenario: Option<Scenario>,

    /// Define which API to use to fetch cell data
    #[arg(short('a'), value_enum, required = false)]
    pub cellapi: Option<CellApiConfig>,

    /// Config for fetching data from Milesight router API
    #[command(flatten)]
    pub milesight: Option<MilesightArgs>,

    /// Config for fetching data from DevicePublisher app API
    #[command(flatten)]
    pub devicepublisher: Option<DevicePublisherArgs>,

    #[command(flatten)]
    pub ngscope: Option<NgScopeArgs>,

    #[command(flatten)]
    pub rntimatching: Option<RntiMatchingArgs>,

    #[command(flatten)]
    pub model: Option<ModelArgs>,

    #[command(flatten)]
    pub log: Option<LogArgs>,

    #[command(flatten)]
    pub download: Option<DownloadArgs>,

    /// Print additional information in the terminal
    #[arg(short('v'), long, required = false)]
    pub verbose: Option<bool>,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum Scenario {
    /// Track UE and send estimated capacity
    TrackUeAndEstimateTransportCapacity,
    /// Do not send anything or try to identify the UE's traffic - just collect the cell's DCI data
    TrackCellDciOnly,
    /// Perform a measurement by downloading data and collecting connection information
    PerformMeasurement,
}

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Serialize, Deserialize)]
pub enum CellApiConfig {
    /// Use a Milesight router as cell data API
    Milesight,
    /// Use the DevicePublisher app as cell data API
    DevicePublisher,
}

#[derive(Clone, Debug)]
pub enum FlattenedCellApiConfig {
    Milesight(FlattenedMilesightArgs),
    DevicePublisher(FlattenedDevicePublisherArgs),
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MilesightArgs {
    /// URL to fetch data from
    #[arg(long, required = false)]
    pub milesight_address: Option<String>,
    /// username for login
    #[arg(long, required = false)]
    pub milesight_user: Option<String>,
    /// authentication: Base64 encoded string (NOT the password base64 encoded, you need to get this through wireshark)
    #[arg(long, required = false)]
    pub milesight_auth: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FlattenedMilesightArgs {
    pub milesight_address: String,
    pub milesight_user: String,
    pub milesight_auth: String,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevicePublisherArgs {
    /// Base address of DevicePublisher
    #[arg(long, required = false)]
    pub devpub_address: Option<String>,
    /// Some authentication
    #[arg(long, required = false)]
    pub devpub_auth: Option<String>,
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct FlattenedDevicePublisherArgs {
    pub devpub_address: String,
    pub devpub_auth: String,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NgScopeArgs {
    /// Path to the ng-scope executable
    #[arg(long, required = false)]
    pub ng_path: Option<String>,

    /// Local UE Cell Tracker address to communicate with NG-Scope (addr:port)
    #[arg(long, required = false)]
    pub ng_local_addr: Option<String>,

    /// Address of the NG-Scope remote interface (addr:port)
    #[arg(long, required = false)]
    pub ng_server_addr: Option<String>,

    /// SDR configuration
    #[command(flatten)]
    pub ng_sdr_config: Option<NgScopeSdrConfigArgs>,

    /// Filepath for stdout + stderr logging of the NG-Scope process
    #[arg(long, required = false)]
    pub ng_log_file: Option<String>,

    /// If true, UE Cell Tracker starts its own NG-Scope instance
    #[arg(long, required = false)]
    pub ng_start_process: Option<bool>,

    /// Log DCI and general cell data information
    #[arg(long, required = false)]
    pub ng_log_dci: Option<bool>,

    /// Determine the number of DCIs contained in a single log file
    #[arg(long, required = false)]
    pub ng_log_dci_batch_size: Option<u64>,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NgScopeSdrConfigArgs {
    /// SDR A
    #[command(flatten)]
    pub ng_sdr_a: Option<NgScopeSdrConfigArgsA>,

    /// SDR B
    #[command(flatten)]
    pub ng_sdr_b: Option<NgScopeSdrConfigArgsB>,

    /// SDR C
    #[command(flatten)]
    pub ng_sdr_c: Option<NgScopeSdrConfigArgsC>,
}


#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NgScopeSdrConfigArgsA {
    /// SDR USB serial identifier
    #[arg(long, required = false)]
    ng_sdr_a_serial: Option<String>,

    /// NG-Scope cell selection parameter
    #[arg(long, required = false)]
    ng_sdr_a_n_id: Option<i16>,
}

#[derive(Args, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NgScopeSdrConfigArgsB {
    /// SDR USB serial identifier
    #[arg(long, required = false)]
    ng_sdr_b_serial: Option<String>,

    /// NG-Scope cell selection parameter
    #[arg(long, required = false)]
    ng_sdr_b_n_id: Option<i16>,
}

#[derive(Args, Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct NgScopeSdrConfigArgsC {
    /// SDR USB serial identifier
    #[arg(long, required = false)]
    ng_sdr_c_serial: Option<String>,

    /// NG-Scope cell selection parameter
    #[arg(long, required = false)]
    ng_sdr_c_n_id: Option<i16>,
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeArgs {
    pub ng_path: String,
    pub ng_local_addr: String,
    pub ng_server_addr: String,
    pub ng_sdr_config: FlattenedNgScopeSdrConfigArgs,
    pub ng_log_file: Option<String>,
    pub ng_start_process: bool,
    pub ng_log_dci: bool,
    pub ng_log_dci_batch_size: u64,
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeSdrConfigArgs {
    pub ng_sdr_a: FlattenedNgScopeSdrConfigArgsA,
    pub ng_sdr_b: Option<FlattenedNgScopeSdrConfigArgsB>,
    pub ng_sdr_c: Option<FlattenedNgScopeSdrConfigArgsC>,
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeSdrConfigArgsA {
    pub ng_sdr_a_serial: String,
    pub ng_sdr_a_n_id: i16,
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeSdrConfigArgsB {
    pub ng_sdr_b_serial: String,
    pub ng_sdr_b_n_id: i16,
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeSdrConfigArgsC {
    pub ng_sdr_c_serial: String,
    pub ng_sdr_c_n_id: i16,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RntiMatchingArgs {
    /// Local UE Cell Tracker address to generate RNTI matching traffic (addr:port)
    #[arg(long, required = false)]
    pub matching_local_addr: Option<String>,

    /// List of traffic patterns (iterates all given patterns)
    #[arg(long, value_enum, required = false)]
    pub matching_traffic_pattern: Option<Vec<RntiMatchingTrafficPatternType>>,

    /// The destination address which the traffic pattern is sent to
    #[arg(long, required = false)]
    pub matching_traffic_destination: Option<String>,

    /// Log RNTI matching traffic and features
    #[arg(long, required = false)]
    pub matching_log_traffic: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct FlattenedRntiMatchingArgs {
    pub matching_local_addr: String,
    pub matching_traffic_pattern: Vec<RntiMatchingTrafficPatternType>,
    pub matching_traffic_destination: String,
    pub matching_log_traffic: bool,
}

#[derive(Copy, Clone, PartialEq, PartialOrd, ValueEnum, Debug, Serialize, Deserialize)]
pub enum DynamicValue {
    FixedMs,
    RttFactor,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelArgs {
    /// Interval in which the Metric is calculated and sent to the destination
    #[arg(long, required = false)]
    pub model_send_metric_interval_value: Option<f64>,

    /// Metric interval type (Rtt-factor or fixed)
    #[arg(long, value_enum, required = false)]
    pub model_send_metric_interval_type: Option<DynamicValue>,

    /// Number of DCIs to base the Metric calculation on
    #[arg(long, value_enum, required = false)]
    pub model_metric_smoothing_size_value: Option<f64>,

    /// Metric smoothing type (Rtt-factor or fixed)
    #[arg(long, value_enum, required = false)]
    pub model_metric_smoothing_size_type: Option<DynamicValue>,

    /// Log Metric and calculation basis
    #[arg(long, required = false)]
    pub model_log_metric: Option<bool>,
}

#[derive(Clone, Debug)]
pub struct FlattenedModelArgs {
    pub model_send_metric_interval_value: f64,
    pub model_send_metric_interval_type: DynamicValue,
    pub model_metric_smoothing_size_value: f64,
    pub model_metric_smoothing_size_type: DynamicValue,
    pub model_log_metric: bool,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LogArgs {
    /// Base directory for logging
    #[arg(long, required = false)]
    pub log_base_dir: Option<String>,
}

#[derive(Clone, Debug)]
pub struct FlattenedLogArgs {
    pub log_base_dir: String,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DownloadArgs {
    /// Base target address inluding host and port
    pub download_base_addr: Option<String>,
    /// List of paths to call on the base address
    pub download_paths: Option<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct FlattenedDownloadArgs {
    pub download_base_addr: String,
    pub download_paths: Vec<String>,
}

impl default::Default for Arguments {
    fn default() -> Self {
        Arguments {
            scenario: Some(DEFAULT_SCENARIO),
            verbose: Some(DEFAULT_VERBOSE),
            cellapi: Some(DEFAULT_CELL_API),
            milesight: Some(MilesightArgs::default()),
            devicepublisher: Some(DevicePublisherArgs::default()),
            ngscope: Some(NgScopeArgs::default()),
            rntimatching: Some(RntiMatchingArgs::default()),
            model: Some(ModelArgs::default()),
            log: Some(LogArgs::default()),
            download: Some(DownloadArgs::default()),
        }
    }
}

impl Arguments {
    /// Build Arguments struct
    pub fn build() -> Result<Self, Box<dyn Error>> {
        let app: Command = Arguments::command();
        let app_name: &str = app.get_name();
        let user_args = Arguments::parse();
        let config_args: Arguments = confy::load(app_name, None)?;
        let combined_args = user_args.merge_with(config_args);
        let printed_args = combined_args.print_config_file(app_name)?;
        Ok(printed_args)
    }

    /// Merge Arguments with another Arguments struct.
    /// !!! This fills None values with their ::defaults !!!
    /// other > self > defaults
    fn merge_with(mut self, other: Arguments) -> Self {
        // Simple types
        self.scenario = self.scenario.or(other.scenario).or(Some(DEFAULT_SCENARIO));
        self.cellapi = self.cellapi.or(other.cellapi).or(Some(DEFAULT_CELL_API));
        self.verbose = self.verbose.or(other.verbose).or(Some(DEFAULT_VERBOSE));
        // Struct types
        self.fill_milesight(&other);
        self.fill_devicepublisher(&other);
        self.fill_ngscope(&other);
        self.fill_rntimatching(&other);
        self.fill_download(&other);
        self.fill_model(&other);
        self.fill_log(&other);

        self
    }

    fn fill_milesight(&mut self, other: &Arguments) {
        let defaults = MilesightArgs::default();
        let defaults_with_self = defaults.merge_with(self.milesight.clone());
        let combined = defaults_with_self.merge_with(other.milesight.clone());
        self.milesight = Some(combined);
    }

    fn fill_devicepublisher(&mut self, other: &Arguments) {
        let defaults = DevicePublisherArgs::default();
        let defaults_with_self = defaults.merge_with(self.devicepublisher.clone());
        let combined = defaults_with_self.merge_with(other.devicepublisher.clone());
        self.devicepublisher = Some(combined);
    }

    fn fill_ngscope(&mut self, other: &Arguments) {
        let defaults = NgScopeArgs::default();
        let defaults_with_self = defaults.merge_with(self.ngscope.clone());
        let combined = defaults_with_self.merge_with(other.ngscope.clone());
        self.ngscope = Some(combined);
    }

    fn fill_rntimatching(&mut self, other: &Arguments) {
        let defaults = RntiMatchingArgs::default();
        let defaults_with_self = defaults.merge_with(self.rntimatching.clone());
        let combined = defaults_with_self.merge_with(other.rntimatching.clone());
        self.rntimatching = Some(combined);
    }

    fn fill_model(&mut self, other: &Arguments) {
        let defaults = ModelArgs::default();
        let defaults_with_self = defaults.merge_with(self.model.clone());
        let combined = defaults_with_self.merge_with(other.model.clone());
        self.model = Some(combined);
    }

    fn fill_download(&mut self, other: &Arguments) {
        let defaults = DownloadArgs::default();
        let defaults_with_self = defaults.merge_with(self.download.clone());
        let combined = defaults_with_self.merge_with(other.download.clone());
        self.download = Some(combined);
    }

    fn fill_log(&mut self, other: &Arguments) {
        let defaults = LogArgs::default();
        let defaults_with_self = defaults.merge_with(self.log.clone());
        let combined = defaults_with_self.merge_with(other.log.clone());
        self.log = Some(combined);
    }

    /// Save changes made to a configuration object
    #[allow(dead_code)]
    fn set_config_file(self, app_name: &str) -> Result<Self, Box<dyn Error>> {
        let default_args: Arguments = Default::default();
        confy::store(app_name, None, default_args)?;
        Ok(self)
    }

    /// Print configuration file path and its contents
    fn print_config_file(self, app_name: &str) -> Result<Self, Box<dyn Error>> {
        if self.verbose.unwrap_or(true) {
            let file_path: PathBuf = confy::get_configuration_file_path(app_name, None)?;
            print_info(&format!(
                "DEBUG [parse] Configuration file: '{}'",
                file_path.display()
            ));

            let yaml: String = serde_yaml::to_string(&self)?;
            print_info(&format!("\t{}", yaml.replace('\n', "\n\t")));
        }

        Ok(self)
    }
}

/*  --------------  */
/*     Defaults     */
/*  --------------  */
impl default::Default for MilesightArgs  {
    fn default() -> Self {
        MilesightArgs {
            milesight_address: Some(DEFAULT_MILESIGHT_ADDRESS.to_string()),
            milesight_user: Some(DEFAULT_MILESIGHT_USER.to_string()),
            milesight_auth: Some(DEFAULT_MILESIGHT_AUTH.to_string()),
        }
    }
}

impl default::Default for DevicePublisherArgs  {
    fn default() -> Self {
        DevicePublisherArgs {
            devpub_address: Some(DEFAULT_DEVPUB_ADDRESS.to_string()),
            devpub_auth: Some(DEFAULT_DEVPUB_AUTH.to_string()),
        }
    }
}

impl default::Default for NgScopeArgs  {
    fn default() -> Self {
        NgScopeArgs {
            ng_path: Some(DEFAULT_NG_PATH.to_string()),
            ng_local_addr: Some(DEFAULT_NG_LOCAL_ADDR.to_string()),
            ng_server_addr: Some(DEFAULT_NG_SERVER_ADDR.to_string()),
            ng_log_file: Some(DEFAULT_NG_LOG_FILE.to_string()),
            ng_start_process: Some(DEFAULT_NG_START_PROCESS),
            ng_log_dci: Some(DEFAULT_NG_LOG_DCI),
            ng_log_dci_batch_size: Some(DEFAULT_NG_LOG_DCI_BATCH_SIZE),
            ng_sdr_config: Some(NgScopeSdrConfigArgs::default()),
        }
    }
}

impl default::Default for NgScopeSdrConfigArgs {
    fn default() -> Self {
        NgScopeSdrConfigArgs {
            ng_sdr_a: Some(NgScopeSdrConfigArgsA::default()),
            ng_sdr_b: None,
            ng_sdr_c: None,
        }
    }
}

impl default::Default for NgScopeSdrConfigArgsA {
    fn default() -> Self {
        NgScopeSdrConfigArgsA {
            ng_sdr_a_serial: Some(DEFAULT_NG_SDR_A_SERIAL.to_string()),
            ng_sdr_a_n_id: Some(DEFAULT_NG_SDR_A_N_ID),
        }
    }
}

impl default::Default for RntiMatchingArgs {
    fn default() -> Self {
        RntiMatchingArgs {
            matching_local_addr: Some(DEFAULT_MATCHING_LOCAL_ADDR.to_string()),
            matching_traffic_pattern: Some(DEFAULT_MATCHING_TRAFFIC_PATTERN.to_vec()),
            matching_traffic_destination: Some(DEFAULT_MATCHING_TRAFFIC_DEST.to_string()),
            matching_log_traffic: Some(DEFAULT_MATCHING_LOG_TRAFFIC),
        }
    }
}

impl default::Default for ModelArgs {
    fn default() -> Self {
        ModelArgs {
            model_send_metric_interval_value: Some(DEFAULT_MODEL_INTERVAL_VALUE),
            model_send_metric_interval_type: Some(DEFAULT_MODEL_INTERVAL_TYPE),
            model_metric_smoothing_size_value: Some(DEFAULT_MODEL_SMOOTHING_VALUE),
            model_metric_smoothing_size_type: Some(DEFAULT_MODEL_SMOOTHING_TYPE),
            model_log_metric: Some(DEFAULT_MODEL_LOG_METRIC),
        }
    }
}

impl default::Default for DownloadArgs {
    fn default() -> Self {
        DownloadArgs {
            download_base_addr: Some(DEFAULT_DOWNLOAD_BASE_ADDR.to_string()),
            download_paths: Some(DEFAULT_DOWNLOAD_PATHS
                .iter()
                .map(|path| path.to_string())
                .collect()),
        }
    }
}

impl default::Default for LogArgs {
    fn default() -> Self {
        LogArgs {
            log_base_dir: Some(DEFAULT_LOG_BASE_DIR.to_string()),
        }
    }
}

/*  --------------  */
/*  Merge Helpers   */
/*  --------------  */
impl MilesightArgs {
    fn merge_with(mut self, other: Option<MilesightArgs>) -> Self {
        if let Some(other) = other {
            self.milesight_address.update_if_some(other.milesight_address);
            self.milesight_user.update_if_some(other.milesight_user);
            self.milesight_auth.update_if_some(other.milesight_auth);
        }
        self
    }
}

impl DevicePublisherArgs {
    fn merge_with(mut self, other: Option<DevicePublisherArgs>) -> Self {
        if let Some(other) = other {
            self.devpub_address.update_if_some(other.devpub_address);
            self.devpub_auth.update_if_some(other.devpub_auth);
        }
        self
    }
}

impl NgScopeArgs {
    fn merge_with(mut self, other: Option<NgScopeArgs>) -> Self {
        if let Some(other) = other {
            self.ng_path.update_if_some(other.ng_path);
            self.ng_local_addr.update_if_some(other.ng_local_addr);
            self.ng_server_addr.update_if_some(other.ng_server_addr);
            self.ng_log_file.update_if_some(other.ng_log_file);
            self.ng_start_process.update_if_some(other.ng_start_process);
            self.ng_log_dci.update_if_some(other.ng_log_dci);
            self.ng_log_dci_batch_size.update_if_some(other.ng_log_dci_batch_size);
            self.ng_sdr_config = if let Some(ng_sdr_config) = self.ng_sdr_config {
                Some(ng_sdr_config.merge_with(other.ng_sdr_config))
            } else {
                other.ng_sdr_config
            };
        }
        self
    }
}

impl NgScopeSdrConfigArgs {
    fn merge_with(mut self, other: Option<NgScopeSdrConfigArgs>) -> Self {
        if let Some(other) = other {
            self.ng_sdr_a = match (self.ng_sdr_a, other.ng_sdr_a) {
                (Some(sdr_a), Some(other_a)) => { Some(sdr_a.merge_with(Some(other_a))) }
                (None, Some(other_a)) => Some(other_a),
                (some_a, None) => some_a,
            };
            self.ng_sdr_b = match (self.ng_sdr_b, other.ng_sdr_b) {
                (Some(sdr_b), Some(other_b)) => { Some(sdr_b.merge_with(Some(other_b))) }
                (None, Some(other_b)) => Some(other_b),
                (some_b, None) => some_b,
            };
            self.ng_sdr_c = match (self.ng_sdr_c, other.ng_sdr_c) {
                (Some(sdr_c), Some(other_c)) => { Some(sdr_c.merge_with(Some(other_c))) }
                (None, Some(other_c)) => Some(other_c),
                (some_c, None) => some_c,
            };
        }
        self
    }
}

impl NgScopeSdrConfigArgsA {
    fn merge_with(mut self, other: Option<NgScopeSdrConfigArgsA>) -> Self {
        if let Some(other) = other {
            self.ng_sdr_a_serial.update_if_some(other.ng_sdr_a_serial);
            self.ng_sdr_a_n_id.update_if_some(other.ng_sdr_a_n_id);
        }
        self
    }
}

impl NgScopeSdrConfigArgsB {
    fn merge_with(mut self, other: Option<NgScopeSdrConfigArgsB>) -> Self {
        if let Some(other) = other {
            self.ng_sdr_b_serial.update_if_some(other.ng_sdr_b_serial);
            self.ng_sdr_b_n_id.update_if_some(other.ng_sdr_b_n_id);
        }
        self
    }
}

impl NgScopeSdrConfigArgsC {
    fn merge_with(mut self, other: Option<NgScopeSdrConfigArgsC>) -> Self {
        if let Some(other) = other {
            self.ng_sdr_c_serial.update_if_some(other.ng_sdr_c_serial);
            self.ng_sdr_c_n_id.update_if_some(other.ng_sdr_c_n_id);
        }
        self
    }
}

impl RntiMatchingArgs {
    fn merge_with(mut self, other: Option<RntiMatchingArgs>) -> Self {
        if let Some(other) = other {
            self.matching_local_addr.update_if_some(other.matching_local_addr);
            self.matching_traffic_pattern.update_if_some(other.matching_traffic_pattern);
            self.matching_traffic_destination.update_if_some(other.matching_traffic_destination);
            self.matching_log_traffic.update_if_some(other.matching_log_traffic);
        }
        self
    }
}

impl ModelArgs {
    fn merge_with(mut self, other: Option<ModelArgs>) -> Self {
        if let Some(other) = other {
            self.model_send_metric_interval_value.update_if_some(other.model_send_metric_interval_value);
            self.model_send_metric_interval_type.update_if_some(other.model_send_metric_interval_type);
            self.model_metric_smoothing_size_value.update_if_some(other.model_metric_smoothing_size_value);
            self.model_metric_smoothing_size_type.update_if_some(other.model_metric_smoothing_size_type);
            self.model_log_metric.update_if_some(other.model_log_metric);
        }
        self
    }
}

impl DownloadArgs {
    fn merge_with(mut self, other: Option<DownloadArgs>) -> Self {
        if let Some(other) = other {
            self.download_base_addr.update_if_some(other.download_base_addr);
            self.download_paths.update_if_some(other.download_paths);
        }
        self
    }
}

impl LogArgs {
    fn merge_with(mut self, other: Option<LogArgs>) -> Self {
        if let Some(other) = other {
            self.log_base_dir.update_if_some(other.log_base_dir);
        }
        self
    }
}

/*  --------------                     */
/*  Unpacking (non-optional) helpers   */
/*  --------------                     */
impl FlattenedCellApiConfig {
    pub fn from_unflattened(
        cell_api: CellApiConfig,
        milesight_args: MilesightArgs,
        devicepublisher_args: DevicePublisherArgs,
    ) -> Result<FlattenedCellApiConfig> {
        match cell_api {
            CellApiConfig::Milesight => {
                Ok(FlattenedCellApiConfig::Milesight(FlattenedMilesightArgs {
                    milesight_address: milesight_args.milesight_address.unwrap(),
                    milesight_user: milesight_args.milesight_user.unwrap(),
                    milesight_auth: milesight_args.milesight_auth.unwrap(),
                }))
            }
            CellApiConfig::DevicePublisher => Ok(FlattenedCellApiConfig::DevicePublisher(
                FlattenedDevicePublisherArgs {
                    devpub_address: devicepublisher_args.devpub_address.unwrap(),
                    devpub_auth: devicepublisher_args.devpub_auth.unwrap(),
                },
            )),
        }
    }
}

impl FlattenedNgScopeArgs {
    pub fn from_unflattened(ng_args: NgScopeArgs) -> Result<FlattenedNgScopeArgs> {
        Ok(FlattenedNgScopeArgs {
            ng_path: ng_args.ng_path.unwrap(),
            ng_local_addr: ng_args.ng_local_addr.unwrap(),
            ng_server_addr: ng_args.ng_server_addr.unwrap(),
            ng_start_process: ng_args.ng_start_process.unwrap(),
            ng_log_file: ng_args.ng_log_file,
            ng_log_dci: ng_args.ng_log_dci.unwrap(),
            ng_log_dci_batch_size: ng_args.ng_log_dci_batch_size.unwrap(),
            ng_sdr_config: FlattenedNgScopeSdrConfigArgs::from_unflattened(ng_args.ng_sdr_config.unwrap())?,
        })
    }
}

impl FlattenedNgScopeSdrConfigArgs {
    pub fn from_unflattened(ng_sdr_config: NgScopeSdrConfigArgs) -> Result<FlattenedNgScopeSdrConfigArgs> {
        Ok(FlattenedNgScopeSdrConfigArgs {
            ng_sdr_a: FlattenedNgScopeSdrConfigArgsA::from_unflattened(ng_sdr_config.ng_sdr_a.unwrap())?,
            ng_sdr_b: FlattenedNgScopeSdrConfigArgsB::from_some_unflattened(ng_sdr_config.ng_sdr_b).ok(),
            ng_sdr_c: FlattenedNgScopeSdrConfigArgsC::from_some_unflattened(ng_sdr_config.ng_sdr_c).ok(),
        })
    }
}

impl FlattenedNgScopeSdrConfigArgsA {
    pub fn from_unflattened(ng_sdr_a: NgScopeSdrConfigArgsA) -> Result<FlattenedNgScopeSdrConfigArgsA> {
        Ok(FlattenedNgScopeSdrConfigArgsA {
            ng_sdr_a_serial: ng_sdr_a.ng_sdr_a_serial.expect("ng_sdr_a_serial missing"),
            ng_sdr_a_n_id: ng_sdr_a.ng_sdr_a_n_id.unwrap_or(-1),
        })
    }
}

impl FlattenedNgScopeSdrConfigArgsB {
    pub fn from_some_unflattened(ng_sdr_b_option: Option<NgScopeSdrConfigArgsB>) -> Result<FlattenedNgScopeSdrConfigArgsB> {
        if let Some(ng_sdr_b) = ng_sdr_b_option {
            Ok(FlattenedNgScopeSdrConfigArgsB {
                ng_sdr_b_serial: ng_sdr_b.ng_sdr_b_serial.expect("ng_sdr_b_serial missing"),
                ng_sdr_b_n_id: ng_sdr_b.ng_sdr_b_n_id.unwrap_or(-1),
            })
        }
        else {
            Err(anyhow!("")) // ok, none should've been parsed
        }
    }
}

impl FlattenedNgScopeSdrConfigArgsC {
    pub fn from_some_unflattened(ng_sdr_c_option: Option<NgScopeSdrConfigArgsC>) -> Result<FlattenedNgScopeSdrConfigArgsC> {
        if let Some(ng_sdr_c) = ng_sdr_c_option {
            Ok(FlattenedNgScopeSdrConfigArgsC {
                ng_sdr_c_serial: ng_sdr_c.ng_sdr_c_serial.expect("ng_sdr_c_serial missing"),
                ng_sdr_c_n_id: ng_sdr_c.ng_sdr_c_n_id.unwrap_or(-1),
            })
        }
        else {
            Err(anyhow!("")) // ok, none should've been parsed
        }
    }
}

impl FlattenedRntiMatchingArgs {
    pub fn from_unflattened(rnti_args: RntiMatchingArgs) -> Result<FlattenedRntiMatchingArgs> {
        Ok(FlattenedRntiMatchingArgs {
            matching_local_addr: rnti_args.matching_local_addr.unwrap(),
            matching_traffic_pattern: rnti_args.matching_traffic_pattern.unwrap(),
            matching_traffic_destination: rnti_args.matching_traffic_destination.unwrap(),
            matching_log_traffic: rnti_args.matching_log_traffic.unwrap(),
        })
    }
}

impl FlattenedModelArgs {
    pub fn from_unflattened(model_args: ModelArgs) -> Result<FlattenedModelArgs> {
        Ok(FlattenedModelArgs {
            model_send_metric_interval_value: model_args.model_send_metric_interval_value.unwrap(),
            model_send_metric_interval_type: model_args.model_send_metric_interval_type.unwrap(),
            model_metric_smoothing_size_value: model_args
                .model_metric_smoothing_size_value
                .unwrap(),
            model_metric_smoothing_size_type: model_args.model_metric_smoothing_size_type.unwrap(),
            model_log_metric: model_args.model_log_metric.unwrap(),
        })
    }
}

impl FlattenedLogArgs {
    pub fn from_unflattened(log_args: LogArgs) -> Result<FlattenedLogArgs> {
        Ok(FlattenedLogArgs {
            log_base_dir: log_args.log_base_dir.unwrap(),
        })
    }
}

impl FlattenedDownloadArgs {
    pub fn from_unflattened(download_args: DownloadArgs) -> Result<FlattenedDownloadArgs> {
        Ok(FlattenedDownloadArgs {
            download_base_addr: download_args.download_base_addr.unwrap(),
            download_paths: download_args.download_paths.unwrap(),
        })
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;


    #[test]
    fn test_load_confy_default() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, DEFAULT_CONFIG_STR).expect("Failed to write config");

        let parsed_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");

        let default_args = Arguments::default();
        assert_eq!(parsed_args, default_args);
    }


    #[test]
    fn test_load_confy_partial() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_STR).expect("Failed to write config");

        let parsed_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");

        let partial_args = Arguments {
            cellapi: Some(CellApiConfig::DevicePublisher),
            log: Some(LogArgs {
              log_base_dir: Some("./.logs.ue/".to_string()),
            }),
            scenario: Some(Scenario::TrackUeAndEstimateTransportCapacity),
            milesight: None,
            devicepublisher: None,
            ngscope: None,
            rntimatching: None,
            model: None,
            download: None,
            verbose: None,
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[test]
    fn test_load_confy_partial_ng_sdr() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_NG_SDR_STR).expect("Failed to write config");

        let parsed_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");

        let partial_args = Arguments {
            cellapi: None,
            log: None,
            scenario: None,
            milesight: None,
            devicepublisher: None,
            ngscope: Some(NgScopeArgs {
                ng_path: None,
                ng_local_addr: None,
                ng_server_addr: None,
                ng_sdr_config: Some(NgScopeSdrConfigArgs {
                    ng_sdr_a: Some(NgScopeSdrConfigArgsA {
                        ng_sdr_a_serial: Some("A2C5B62".to_string()),
                        ng_sdr_a_n_id: Some(0),
                    }),
                    ng_sdr_b: Some(NgScopeSdrConfigArgsB {
                        ng_sdr_b_serial: Some("C2B5513".to_string()),
                        ng_sdr_b_n_id: Some(-1),
                    }),
                    ng_sdr_c: Some(NgScopeSdrConfigArgsC {
                        ng_sdr_c_serial: Some("D2D0F61".to_string()),
                        ng_sdr_c_n_id: Some(1),
                    }),
                }),
                ng_log_file: None,
                ng_start_process: None,
                ng_log_dci: None,
                ng_log_dci_batch_size: None,
            }),
            rntimatching: None,
            model: None,
            download: None,
            verbose: None,
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[test]
    fn test_load_confy_partial_ng_sdr_default_n_id() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_NG_SDR_TWO_STR).expect("Failed to write config");

        let parsed_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");

        let partial_args = Arguments {
            cellapi: None,
            log: None,
            scenario: None,
            milesight: None,
            devicepublisher: None,
            ngscope: Some(NgScopeArgs {
                ng_path: None,
                ng_local_addr: None,
                ng_server_addr: None,
                ng_sdr_config: Some(NgScopeSdrConfigArgs {
                    ng_sdr_a: Some(NgScopeSdrConfigArgsA {
                        ng_sdr_a_serial: Some("A2C5B62".to_string()),
                        ng_sdr_a_n_id: None,
                    }),
                    ng_sdr_b: Some(NgScopeSdrConfigArgsB {
                        ng_sdr_b_serial: Some("C2B5513".to_string()),
                        ng_sdr_b_n_id: None,
                    }),
                    ng_sdr_c: None,
                }),
                ng_log_file: None,
                ng_start_process: None,
                ng_log_dci: None,
                ng_log_dci_batch_size: None,
            }),
            rntimatching: None,
            model: None,
            download: None,
            verbose: None,
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[test]
    fn test_parse_default() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, DEFAULT_CONFIG_STR).expect("Failed to write config");

        let loaded_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");
        let parsed_args: Arguments = loaded_args.clone().merge_with(loaded_args); // merging with
                                                                                  // itself applies
                                                                                  // default parameters
        let default_args = Arguments::default();
        assert_eq!(parsed_args, default_args);
    }


    #[test]
    fn test_parse_partial() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_STR).expect("Failed to write config");

        let loaded_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");
        let parsed_args: Arguments = loaded_args.clone().merge_with(loaded_args); // merging with
                                                                                  // itself applies
                                                                                  // default parameters

        let partial_args = Arguments {
            cellapi: Some(CellApiConfig::DevicePublisher),
            log: Some(LogArgs {
              log_base_dir: Some("./.logs.ue/".to_string()),
            }),
            scenario: Some(Scenario::TrackUeAndEstimateTransportCapacity),
            ..Default::default()
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[test]
    fn test_parse_partial_ng_sdr() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_NG_SDR_STR).expect("Failed to write config");

        let loaded_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");
        let parsed_args: Arguments = loaded_args.clone().merge_with(loaded_args); // merging with
                                                                                  // itself applies
                                                                                  // default parameters

        let partial_args = Arguments {
            ngscope: Some(NgScopeArgs {
                ng_sdr_config: Some(NgScopeSdrConfigArgs {
                    ng_sdr_a: Some(NgScopeSdrConfigArgsA {
                        ng_sdr_a_serial: Some("A2C5B62".to_string()),
                        ng_sdr_a_n_id: Some(0),
                    }),
                    ng_sdr_b: Some(NgScopeSdrConfigArgsB {
                        ng_sdr_b_serial: Some("C2B5513".to_string()),
                        ng_sdr_b_n_id: Some(-1),
                    }),
                    ng_sdr_c: Some(NgScopeSdrConfigArgsC {
                        ng_sdr_c_serial: Some("D2D0F61".to_string()),
                        ng_sdr_c_n_id: Some(1),
                    }),
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[test]
    fn test_parse_partial_ng_sdr_default_n_id() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, PARTIAL_CONFIG_NG_SDR_TWO_STR).expect("Failed to write config");

        let loaded_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");
        let parsed_args: Arguments = loaded_args.clone().merge_with(loaded_args); // merging with
                                                                                  // itself applies
                                                                                  // default parameters

        let partial_args = Arguments {
            ngscope: Some(NgScopeArgs {
                ng_sdr_config: Some(NgScopeSdrConfigArgs {
                    ng_sdr_a: Some(NgScopeSdrConfigArgsA {
                        ng_sdr_a_serial: Some("A2C5B62".to_string()),
                        ..Default::default()
                    }),
                    ng_sdr_b: Some(NgScopeSdrConfigArgsB {
                        ng_sdr_b_serial: Some("C2B5513".to_string()),
                        ..Default::default()
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(parsed_args, partial_args);
    }

    #[allow(dead_code)]
    const DEFAULT_CONFIG_STR: &str =
r#"
scenario: TrackUeAndEstimateTransportCapacity
cellapi: Milesight
milesight:
  milesight_address: http://127.0.0.1:8080
  milesight_user: root
  milesight_auth: root-password
devicepublisher:
  devpub_address: 127.0.0.1
  devpub_auth: some_auth
ngscope:
  ng_path: /dev_ws/dependencies/ng-scope/build_x86/ngscope/src/ngscope
  ng_local_addr: 0.0.0.0:9191
  ng_server_addr: 0.0.0.0:6767
  ng_sdr_config:
    ng_sdr_a:
      ng_sdr_a_serial: 3295B62
      ng_sdr_a_n_id: -1
  ng_log_file: ./.ng_scope_log.txt
  ng_start_process: true
  ng_log_dci: true
  ng_log_dci_batch_size: 60000
rntimatching:
  matching_local_addr: 0.0.0.0:9292
  matching_traffic_pattern:
  - A
  matching_traffic_destination: 127.0.0.1:9494
  matching_log_traffic: true
model:
  model_send_metric_interval_value: 1.0
  model_send_metric_interval_type: RttFactor
  model_metric_smoothing_size_value: 1.0
  model_metric_smoothing_size_type: RttFactor
  model_log_metric: true
log:
  log_base_dir: ./.logs.ue/
download:
  download_base_addr: 127.0.0.1:9393
  download_paths:
  - /10s/cubic
  - /10s/bbr
  - /10s/reno
  - /10s/l2b/fair0/init
  - /10s/l2b/fair0/upper
  - /10s/l2b/fair0/init_and_upper
  - /10s/l2b/fair0/direct
  - /10s/l2b/fair1/init
  - /10s/l2b/fair1/upper
  - /10s/l2b/fair1/init_and_upper
  - /10s/l2b/fair1/direct
  - /60s/cubic
  - /60s/bbr
  - /60s/reno
  - /60s/l2b/fair0/init
  - /60s/l2b/fair0/upper
  - /60s/l2b/fair0/init_and_upper
  - /60s/l2b/fair0/direct
  - /60s/l2b/fair1/init
  - /60s/l2b/fair1/upper
  - /60s/l2b/fair1/init_and_upper
  - /60s/l2b/fair1/direct
verbose: true
"#;

    #[allow(dead_code)]
    const PARTIAL_CONFIG_STR: &str =
r#"
scenario: TrackUeAndEstimateTransportCapacity
cellapi: DevicePublisher
log:
  log_base_dir: ./.logs.ue/
"#;

    #[allow(dead_code)]
    const PARTIAL_CONFIG_NG_SDR_STR: &str =
r#"
ngscope:
  ng_sdr_config:
    ng_sdr_a:
      ng_sdr_a_serial: A2C5B62
      ng_sdr_a_n_id: 0
    ng_sdr_b:
      ng_sdr_b_serial: C2B5513
      ng_sdr_b_n_id: -1
    ng_sdr_c:
      ng_sdr_c_serial: D2D0F61
      ng_sdr_c_n_id: 1
"#;

    #[allow(dead_code)]
    const PARTIAL_CONFIG_NG_SDR_TWO_STR: &str =
r#"
ngscope:
  ng_sdr_config:
    ng_sdr_a:
      ng_sdr_a_serial: A2C5B62
    ng_sdr_b:
      ng_sdr_b_serial: C2B5513
"#;


}
