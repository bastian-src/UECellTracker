/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use anyhow::Result;
use clap::{Args, Command, CommandFactory, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

use crate::{logic::traffic_patterns::RntiMatchingTrafficPatternType, util::print_debug};

#[derive(Debug, Clone, PartialEq, Parser, Serialize, Deserialize)]
#[command(author, version, about, long_about = None, next_line_help = true)]
#[command(propagate_version = true)]
pub struct Arguments {
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

    /// Print additional information in the terminal
    #[arg(short('v'), long, required = false)]
    pub verbose: Option<bool>,
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
}

#[derive(Clone, Debug)]
pub struct FlattenedNgScopeArgs {
    pub ng_path: String,
    pub ng_local_addr: String,
    pub ng_server_addr: String,
    pub ng_log_file: Option<String>,
    pub ng_start_process: bool,
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

    /// Log traffic used for matching (in ./logs/rnti_matching*.jsonl)
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
}

#[derive(Clone, Debug)]
pub struct FlattenedModelArgs {
    pub model_send_metric_interval_value: f64,
    pub model_send_metric_interval_type: DynamicValue,
    pub model_metric_smoothing_size_value: f64,
    pub model_metric_smoothing_size_type: DynamicValue,
}

impl default::Default for Arguments {
    fn default() -> Self {
        Arguments {
            verbose: Some(false),
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
        self.verbose = self.verbose.or(config_file.verbose);

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
        })
    }
}
