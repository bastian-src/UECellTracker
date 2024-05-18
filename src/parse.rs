/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use anyhow::Result;
use clap::{Args, Command, CommandFactory, Parser, ValueEnum};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

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

    /// Path to the ng-scope executable
    #[arg(short, long, required = false)]
    pub ngscopepath: Option<String>,
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
pub struct FlattenedDevicePublisherArgs {
    pub devpub_address: String,
    pub devpub_auth: String,
}

impl default::Default for Arguments {
    fn default() -> Self {
        Arguments {
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
            ngscopepath: Some("/dev_ws/dependencies/ng-scope/build_x86/ngscope/src/".to_string()),
            verbose: Some(false),
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
                let printed_args = parsed_config_args
                    .print_config_file(app_name)?;
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
        self.ngscopepath = self.ngscopepath.or(config_file.ngscopepath);
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
            println!("Configuration file: '{}'", file_path.display());

            let yaml: String = serde_yaml::to_string(&self)?;
            println!("\t{}", yaml.replace('\n', "\n\t"));
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
