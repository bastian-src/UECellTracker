/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use anyhow::Result;
use clap::{Args, Command, CommandFactory, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

use crate::{logic::traffic_patterns::RntiMatchingTrafficPatternType, util::print_info};

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

// arguments should be separated into two distinct structs ...
// one for the cli arguments and one for the config file ones
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

//why is only one of the strings optional?
#[derive(Clone, Debug)]
pub struct FlattenedNgScopeArgs {
    pub ng_path: String,
    pub ng_local_addr: String,
    pub ng_server_addr: String,
    pub ng_log_file: Option<String>,
    pub ng_start_process: bool,
    pub ng_log_dci: bool,
    pub ng_log_dci_batch_size: u64,
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
            milesight: Some(MilesightArgs {
                milesight_address: Some(DEFAULT_MILESIGHT_ADDRESS.to_string()),
                milesight_user: Some(DEFAULT_MILESIGHT_USER.to_string()),
                milesight_auth: Some(DEFAULT_MILESIGHT_AUTH.to_string()),
            }),
            devicepublisher: Some(DevicePublisherArgs {
                devpub_address: Some(DEFAULT_DEVPUB_ADDRESS.to_string()),
                devpub_auth: Some(DEFAULT_DEVPUB_AUTH.to_string()),
            }),
            ngscope: Some(NgScopeArgs {
                ng_path: Some(DEFAULT_NG_PATH.to_string()),
                ng_local_addr: Some(DEFAULT_NG_LOCAL_ADDR.to_string()),
                ng_server_addr: Some(DEFAULT_NG_SERVER_ADDR.to_string()),
                ng_log_file: Some(DEFAULT_NG_LOG_FILE.to_string()),
                ng_start_process: Some(DEFAULT_NG_START_PROCESS),
                ng_log_dci: Some(DEFAULT_NG_LOG_DCI),
                ng_log_dci_batch_size: Some(DEFAULT_NG_LOG_DCI_BATCH_SIZE),
            }),
            rntimatching: Some(RntiMatchingArgs {
                matching_local_addr: Some(DEFAULT_MATCHING_LOCAL_ADDR.to_string()),
                matching_traffic_pattern: Some(DEFAULT_MATCHING_TRAFFIC_PATTERN.to_vec()),
                matching_traffic_destination: Some(DEFAULT_MATCHING_TRAFFIC_DEST.to_string()),
                matching_log_traffic: Some(DEFAULT_MATCHING_LOG_TRAFFIC),
            }),
            model: Some(ModelArgs {
                model_send_metric_interval_value: Some(DEFAULT_MODEL_INTERVAL_VALUE),
                model_send_metric_interval_type: Some(DEFAULT_MODEL_INTERVAL_TYPE),
                model_metric_smoothing_size_value: Some(DEFAULT_MODEL_SMOOTHING_VALUE),
                model_metric_smoothing_size_type: Some(DEFAULT_MODEL_SMOOTHING_TYPE),
                model_log_metric: Some(DEFAULT_MODEL_LOG_METRIC),
            }),
            log: Some(LogArgs {
                log_base_dir: Some(DEFAULT_LOG_BASE_DIR.to_string()),
            }),
            download: Some(DownloadArgs {
                download_base_addr: Some(DEFAULT_DOWNLOAD_BASE_ADDR.to_string()),
                download_paths: Some(
                    DEFAULT_DOWNLOAD_PATHS
                        .iter()
                        .map(|path| path.to_string())
                        .collect(),
                ),
            }),
        }
    }
}

impl Arguments {
    /// Build Arguments struct
    pub fn build() -> Result<Self, Box<dyn Error>> {
        let app: Command = Arguments::command();
        let app_name: &str = app.get_name();
        let parsed_args = Arguments::parse();
        match parsed_args.clone().get_config_file(app_name) {
            Ok(parsed_config_args) => {
                let printed_args = parsed_config_args.print_config_file(app_name)?;
                Ok(printed_args)
            }
            Err(_) => {
                let printed_args = parsed_args
                    .set_config_file(app_name)?
                    .print_config_file(app_name)?;
                Ok(printed_args)
            }
        }
    }

    /// Get configuration file.
    /// A new configuration file is created with default values if none exists.
    /// I don't get why we don't modify in-place by using references?
    fn get_config_file(mut self, app_name: &str) -> Result<Self, Box<dyn Error>> {
        let config_file: Arguments = confy::load(app_name, None)?;

        // CLI > Config file > default values
        self.cellapi = self.cellapi.or(config_file.cellapi);
        //self.milesight = self.milesight.or(config_file.milesight);
        //self.devicepublisher = self.devicepublisher.or(config_file.devicepublisher);
        //self.ngscope = self.ngscope.or(config_file.ngscope);
        //self.rntimatching = self.rntimatching.or(config_file.rntimatching);
        //self.model = self.model.or(config_file.model);
        self.log = self.log.or(config_file.log);
        //self.download = self.download.or(config_file.download);
        self.verbose = self.verbose.or(config_file.verbose);
        self.scenario = self.scenario.or(config_file.scenario);
        // when passing arguments via the CLI using clap, we are not using default values (because prior config files have higher prio than default values)
        // which means we sometimes get null values from CLI when a struct is nested
        // the easiest way would probably be to write a wrapper script
        // the clean way would be to implement some kind of merge prioritization
        // we chose to replace the merge above by some filler function
        // nested parts: (exclude log because it only has one field)
        // milesight
        // this borrows the inner struct but not the option/wrapper and doesnt move the config struct
        // the unwrap below consumes the individual parts of the config struct though
        if self.milesight.is_some() {
            if let Some(ref mut milesight) = self.milesight {
                milesight.fill_with_config_file(config_file.milesight.unwrap());
            }
        } else {
            self.milesight = config_file.milesight;
        }
        // devpub
        if self.devicepublisher.is_some() {
            if let Some(ref mut devicepublisher) = self.devicepublisher {
                devicepublisher.fill_with_config_file(config_file.devicepublisher.unwrap());
            }
        } else {
            self.devicepublisher = config_file.devicepublisher;
        }

        // ngscope
        if self.ngscope.is_some() {
            if let Some(ref mut ngscope) = self.ngscope {
                ngscope.fill_with_config_file(config_file.ngscope.unwrap());
            }
        } else {
            self.ngscope = config_file.ngscope;
        }

        // rntimatching
        if self.rntimatching.is_some() {
            if let Some(ref mut rntimatching) = self.rntimatching {
                rntimatching.fill_with_config_file(config_file.rntimatching.unwrap());
            }
        } else {
            self.rntimatching = config_file.rntimatching;
        }

        // model
        if self.model.is_some() {
            if let Some(ref mut model) = self.model {
                model.fill_with_config_file(config_file.model.unwrap());
            }
        } else {
            self.model = config_file.model;
        }

        // download
        if self.download.is_some() {
            if let Some(ref mut download) = self.download {
                download.fill_with_config_file(config_file.download.unwrap());
            }
        } else {
            self.download = config_file.download;
        }

        // probably only need to check download if we are in the PerformMeasurement scenario

        Ok(self)
    }

    /// Save changes made to a configuration object
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

impl MilesightArgs {
    fn fill_with_config_file(&mut self, config_file: MilesightArgs) {
        if self.milesight_address.is_none() {
            self.milesight_address = config_file.milesight_address;
        }
        if self.milesight_user.is_none() {
            self.milesight_user = config_file.milesight_user;
        }
        if self.milesight_auth.is_none() {
            self.milesight_auth = config_file.milesight_auth;
        }
    }
}

impl DevicePublisherArgs {
    fn fill_with_config_file(&mut self, config_file: DevicePublisherArgs) {
        if self.devpub_address.is_none() {
            self.devpub_address = config_file.devpub_address;
        }
        if self.devpub_auth.is_none() {
            self.devpub_auth = config_file.devpub_auth;
        }
    }
}

impl NgScopeArgs {
    fn fill_with_config_file(&mut self, config_file: NgScopeArgs) {
        if self.ng_path.is_none() {
            self.ng_path = config_file.ng_path;
        }
        if self.ng_local_addr.is_none() {
            self.ng_local_addr = config_file.ng_local_addr;
        }
        if self.ng_server_addr.is_none() {
            self.ng_server_addr = config_file.ng_server_addr;
        }
        if self.ng_log_file.is_none() {
            self.ng_log_file = config_file.ng_log_file;
        }
        if self.ng_start_process.is_none() {
            self.ng_start_process = config_file.ng_start_process;
        }
        if self.ng_log_dci.is_none() {
            self.ng_log_dci = config_file.ng_log_dci;
        }
        if self.ng_log_dci_batch_size.is_none() {
            self.ng_log_dci_batch_size = config_file.ng_log_dci_batch_size;
        }
    }
}

impl RntiMatchingArgs {
    fn fill_with_config_file(&mut self, config_file: RntiMatchingArgs) {
        if self.matching_local_addr.is_none() {
            self.matching_local_addr = config_file.matching_local_addr;
        }
        if self.matching_traffic_pattern.is_none() {
            self.matching_traffic_pattern = config_file.matching_traffic_pattern;
        }
        if self.matching_traffic_destination.is_none() {
            self.matching_traffic_destination = config_file.matching_traffic_destination;
        }
        if self.matching_log_traffic.is_none() {
            self.matching_log_traffic = config_file.matching_log_traffic;
        }
    }
}

impl ModelArgs {
    fn fill_with_config_file(&mut self, config_file: ModelArgs) {
        if self.model_send_metric_interval_value.is_none() {
            self.model_send_metric_interval_value = config_file.model_send_metric_interval_value;
        }
        if self.model_send_metric_interval_type.is_none() {
            self.model_send_metric_interval_type = config_file.model_send_metric_interval_type;
        }
        if self.model_metric_smoothing_size_value.is_none() {
            self.model_metric_smoothing_size_value = config_file.model_metric_smoothing_size_value;
        }
        if self.model_metric_smoothing_size_type.is_none() {
            self.model_metric_smoothing_size_type = config_file.model_metric_smoothing_size_type;
        }
    }
}

impl DownloadArgs {
    fn fill_with_config_file(&mut self, config_file: DownloadArgs) {
        if self.download_base_addr.is_none() {
            self.download_base_addr = config_file.download_base_addr;
        }

        if self.download_paths.is_none() {
            self.download_paths = config_file.download_paths;
        }
    }
}

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
        })
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
