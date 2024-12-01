/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use anyhow::Result;
use clap::{Args, Command, CommandFactory, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

use crate::{logic::traffic_patterns::RntiMatchingTrafficPatternType, util::print_debug};

pub const DEFAULT_SCENARIO: Scenario = Scenario::TrackUeAndEstimateTransportCapacity;
pub const DEFAULT_LOG_BASE_DIR: &str = "./.logs.ue/";
pub const DEFAULT_DOWNLOAD_BASE_ADDR: &str = "http://some.addr";
pub const DEFAULT_DOWNLOAD_PATHS: &[&str] = &[
    "/10s/cubic",
    "/10s/bbr",
    "/10s/pbe/fair0/init",
    "/10s/pbe/fair0/upper",
    "/10s/pbe/fair0/init_and_upper",
    "/10s/pbe/fair0/direct",
    "/10s/pbe/fair1/init",
    "/10s/pbe/fair1/upper",
    "/10s/pbe/fair1/init_and_upper",
    "/10s/pbe/fair1/direct",
    "/60s/cubic",
    "/60s/bbr",
    "/60s/pbe/fair0/init",
    "/60s/pbe/fair0/upper",
    "/60s/pbe/fair0/init_and_upper",
    "/60s/pbe/fair0/direct",
    "/60s/pbe/fair1/init",
    "/60s/pbe/fair1/upper",
    "/60s/pbe/fair1/init_and_upper",
    "/60s/pbe/fair1/direct",
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
    /// authentication: Base64 encoded password
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
            verbose: Some(true),
            cellapi: Some(CellApiConfig::Milesight),
            milesight: Some(MilesightArgs {
                milesight_address: Some("http://127.0.0.1".to_string()),
                milesight_user: Some("root".to_string()),
                milesight_auth: Some("root-password".to_string()),
            }),
            devicepublisher: Some(DevicePublisherArgs {
                devpub_address: Some("https://some.address".to_string()),
                devpub_auth: Some("some_auth".to_string()),
            }),
            ngscope: Some(NgScopeArgs {
                ng_path: Some("/dev_ws/dependencies/ng-scope/build_x86/ngscope/src/".to_string()),
                ng_local_addr: Some("0.0.0.0:9191".to_string()),
                ng_server_addr: Some("0.0.0.0:6767".to_string()),
                ng_log_file: Some("./.ng_scope_log.txt".to_string()),
                ng_start_process: Some(true),
                ng_log_dci: Some(true),
                ng_log_dci_batch_size: Some(60000),
            }),
            rntimatching: Some(RntiMatchingArgs {
                matching_local_addr: Some("0.0.0.0:9292".to_string()),
                matching_traffic_pattern: Some(vec![RntiMatchingTrafficPatternType::A]),
                matching_traffic_destination: Some("1.1.1.1:53".to_string()),
                matching_log_traffic: Some(true),
            }),
            model: Some(ModelArgs {
                model_send_metric_interval_value: Some(1.0),
                model_send_metric_interval_type: Some(DynamicValue::RttFactor),
                model_metric_smoothing_size_value: Some(1.0),
                model_metric_smoothing_size_type: Some(DynamicValue::RttFactor),
                model_log_metric: Some(true),
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
    fn get_config_file(mut self, app_name: &str) -> Result<Self, Box<dyn Error>> {
        let config_file: Arguments = confy::load(app_name, None)?;

        self.cellapi = self.cellapi.or(config_file.cellapi);
        self.milesight = self.milesight.or(config_file.milesight);
        self.devicepublisher = self.devicepublisher.or(config_file.devicepublisher);
        self.ngscope = self.ngscope.or(config_file.ngscope);
        self.rntimatching = self.rntimatching.or(config_file.rntimatching);
        self.model = self.model.or(config_file.model);
        self.log = self.log.or(config_file.log);
        self.download = self.download.or(config_file.download);
        self.verbose = self.verbose.or(config_file.verbose);
        self.scenario = self.scenario.or(config_file.scenario);

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
            print_debug(&format!(
                "DEBUG [parse] Configuration file: '{}'",
                file_path.display()
            ));

            let yaml: String = serde_yaml::to_string(&self)?;
            print_debug(&format!("\t{}", yaml.replace('\n', "\n\t")));
        }

        Ok(self)
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



#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;


    #[test]
    fn test_parse_default() {
        let temp_dir = tempdir().expect("Failed to create temp dir");
        let config_path = temp_dir.path().join("ue-cell-tracker.yaml");
        fs::write(&config_path, DEFAULT_CONFIG_STR).expect("Failed to write config");

        let parsed_args: Arguments = confy::load_path(&config_path)
            .expect("Error loading ue-cell-tracker config");

        let default_args = Arguments::default();
        assert_eq!(parsed_args, default_args);
    }


    #[test]
    fn test_parse_partial() {
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


    #[allow(dead_code)]
    const DEFAULT_CONFIG_STR: &str =
r#"scenario: TrackUeAndEstimateTransportCapacity
cellapi: Milesight
milesight:
  milesight_address: http://127.0.0.1
  milesight_user: root
  milesight_auth: root-password
devicepublisher:
  devpub_address: https://some.address
  devpub_auth: some_auth
ngscope:
  ng_path: /dev_ws/dependencies/ng-scope/build_x86/ngscope/src/
  ng_local_addr: 0.0.0.0:9191
  ng_server_addr: 0.0.0.0:6767
  ng_log_file: ./.ng_scope_log.txt
  ng_start_process: true
  ng_log_dci: true
  ng_log_dci_batch_size: 60000
rntimatching:
  matching_local_addr: 0.0.0.0:9292
  matching_traffic_pattern:
  - A
  matching_traffic_destination: 1.1.1.1:53
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
  download_base_addr: http://some.addr
  download_paths:
  - /10s/cubic
  - /10s/bbr
  - /10s/pbe/fair0/init
  - /10s/pbe/fair0/upper
  - /10s/pbe/fair0/init_and_upper
  - /10s/pbe/fair0/direct
  - /10s/pbe/fair1/init
  - /10s/pbe/fair1/upper
  - /10s/pbe/fair1/init_and_upper
  - /10s/pbe/fair1/direct
  - /60s/cubic
  - /60s/bbr
  - /60s/pbe/fair0/init
  - /60s/pbe/fair0/upper
  - /60s/pbe/fair0/init_and_upper
  - /60s/pbe/fair0/direct
  - /60s/pbe/fair1/init
  - /60s/pbe/fair1/upper
  - /60s/pbe/fair1/init_and_upper
  - /60s/pbe/fair1/direct
verbose: true"#;

    #[allow(dead_code)]
    const PARTIAL_CONFIG_STR: &str =
r#"scenario: TrackUeAndEstimateTransportCapacity
cellapi: DevicePublisher
log:
  log_base_dir: ./.logs.ue/"#;

}
