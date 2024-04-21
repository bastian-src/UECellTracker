/// Credits: https://stackoverflow.com/questions/55133351/is-there-a-way-to-get-clap-to-use-default-values-from-a-file
use clap::{Args, Command, ValueEnum, CommandFactory, Parser};
use serde::{Deserialize, Serialize};
use std::{default, error::Error, path::PathBuf};

#[derive(Debug, Clone, PartialEq, Parser, Serialize, Deserialize)]
#[command(author, version, about, long_about = None, next_line_help = true)]
#[command(propagate_version = true)]
pub struct Arguments {
    /// Define which API to use to fetch cell data
    #[arg(value_enum, required=false)]
    pub cellapi: Option<CellApiConfig>,

    /// Config for fetching data from Milesight router API
    #[command(flatten)]
    pub milesight: Option<MilesightArgs>,

    /// Config for fetching data from DevicePublisher app API
    #[command(flatten)]
    pub devicepublisher: Option<DevicePublisherArgs>,
 
    /// Path to the ng-scope executable
    #[arg(required=false)]
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

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MilesightArgs {
    /// URL to fetch data from
    pub milesight_url: Option<String>,
    /// Base64 encoded auth
    pub milesight_auth: Option<String>,
}

#[derive(Args, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DevicePublisherArgs {
    /// Base address of DevicePublisher
    pub devpub_address: Option<String>,
    /// Some authentication
    pub devpub_auth: Option<String>,
}

impl default::Default for Arguments {
    fn default() -> Self {
        Arguments {
            cellapi: Some(CellApiConfig::Milesight),
            milesight: Some(MilesightArgs {
                milesight_url: Some("https://some.address.com/with/path".to_string()),
                milesight_auth: Some("some_auth".to_string()),
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

        let args: Arguments = Arguments::parse()
            .get_config_file(app_name)?
            .set_config_file(app_name)?
            .print_config_file(app_name)?;

        Ok(args)
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

