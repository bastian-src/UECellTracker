use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::option::Option;

#[derive(Serialize, Deserialize, Clone, Debug)]
#[allow(non_snake_case)]
pub struct NgScopeConfigRfDev {
    pub rf_freq: u64,
    pub N_id_2: i16,
    pub rf_args: String,
    pub nof_thread: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub disable_plot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_dl: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_ul: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub log_phich: Option<bool>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NgScopeConfigDciLog {
    pub nof_cell: u16,
    pub log_ul: bool,
    pub log_dl: bool,
    pub log_interval: u16,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct NgScopeConfig {
    pub nof_rf_dev: u16,
    pub rnti: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remote_enable: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_single_ue: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decode_sib: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dci_logs_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sib_logs_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dci_log_config: Option<NgScopeConfigDciLog>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rf_config0: Option<NgScopeConfigRfDev>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rf_config1: Option<NgScopeConfigRfDev>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rf_config2: Option<NgScopeConfigRfDev>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rf_config3: Option<NgScopeConfigRfDev>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rf_config4: Option<NgScopeConfigRfDev>,
}

impl Default for NgScopeConfigRfDev {
    fn default() -> Self {
        NgScopeConfigRfDev {
            rf_freq: 796000000,
            N_id_2: -1,
            rf_args: "serial=3295B62".to_string(),
            nof_thread: 4,
            disable_plot: None,
            log_dl: None,
            log_ul: None,
            log_phich: None,
        }
    }
}

impl Default for NgScopeConfigDciLog {
    fn default() -> Self {
        NgScopeConfigDciLog {
            nof_cell: 1,
            log_ul: false,
            log_dl: false,
            log_interval: 5,
        }
    }
}

impl Default for NgScopeConfig {
    fn default() -> Self {
        NgScopeConfig {
            nof_rf_dev: 1,
            rnti: 0xffff,
            remote_enable: None,
            decode_single_ue: None,
            decode_sib: None,
            dci_logs_path: None,
            sib_logs_path: None,
            dci_log_config: Some(NgScopeConfigDciLog::default()),
            rf_config0: Some(NgScopeConfigRfDev::default()),
            rf_config1: None,
            rf_config2: None,
            rf_config3: None,
            rf_config4: None,
        }
    }
}

pub fn read_config(file_path: &str) -> Result<NgScopeConfig> {
    serde_libconfig::from_file::<NgScopeConfig>(file_path)
}

pub fn write_config(config: &NgScopeConfig, file_path: &str) -> Result<()> {
    serde_libconfig::to_file::<NgScopeConfig>(config, file_path)
}

// TODO: serialize and deserialize config:
// https://github.com/JoNil/libconfig-rs/blob/master/src/lib.rs#L131
// https://crates.io/crates/libconfig-rs

#[cfg(test)]

mod tests {
    use super::*;

    #[allow(dead_code)]
    const COMPLEX_CONFIG_STR: &str = r#"nof_rf_dev = 1;
// rnti=21095;
// rnti=4885; // = 0x1315
// rnti             = 0xffff;
//rnti             = 49085;
rnti             = 19015;
decode_SIB       = true;
disable_plot     = true;
remote_enable    = true;
decode_single_ue = false;


rf_config0 = {
    //rf_freq   	= 2680000000L;
    //rf_freq   	= 2140000000L;
    //rf_freq   	    = 806000000L;
    rf_freq   	    = 796000000L; // runs way more consistent with this cell.
    //rf_freq   	= 2120000000L;
    N_id_2  		= -1;
    rf_args 		= "serial=3295B62";
    nof_thread  	= 4;
    disable_plot    = true;
    log_dl  		= true;
    log_ul  		= true;
}

dci_log_config = {
    log_dl  = true;
    log_ul  = true;
}"#;

    #[allow(dead_code)]
    const EASY_CONFIG_STR: &str = r#"nof_rf_dev = 1;
rnti             = 19015;
decode_SIB       = true;
disable_plot     = true;
remote_enable    = true;
decode_single_ue = false;

rf_config0 = {
    rf_freq   	    = 796000000L;
    N_id_2  		= -1;
    rf_args 		= "serial=3295B62";
    nof_thread  	= 4;
    disable_plot    = true;
    log_dl  		= true;
    log_ul  		= true;
}

dci_log_config = {
    log_dl  = true;
    log_ul  = true;
}"#;

    const DEFAULT_CONFIG_STR: &str = r#"nof_rf_dev = 1;
rnti = 65535;
dci_log_config = {
    nof_cell = 1;
    log_ul = false;
    log_dl = false;
    log_interval = 5;
};
rf_config0 = {
    rf_freq = 796000000;
    N_id_2 = -1;
    rf_args = "serial=3295B62";
    nof_thread = 4;
};"#;

    #[test]
    fn test_config_ser() {
        let config = NgScopeConfig::default();
        let config_str = serde_libconfig::to_string::<NgScopeConfig>(&config).unwrap();
        assert_eq!(config_str, DEFAULT_CONFIG_STR)
    }

    // #[test]
    // fn test_config_de() {
    //     let config = serde_libconfig::from_string::<NgScopeConfig>(DUMMY_CONFIG_STR);
    //     // HERE: Debug errors (implement serde_libconfig properly)
    //     assert!(config.is_ok())
    // }

    // #[test]
    // fn test_config_de_ignores_comments() {
    //     let config = serde_libconfig::from_string::<NgScopeConfig>(DUMMY_CONFIG_STR).unwrap();
    //     assert_eq!(config.nof_rf_dev, 1)
    // }

    // #[test]
    // fn test_easy_config_de() {
    //     let dummy_config = NgScopeConfig::default();
    //     let config = serde_libconfig::from_string::<NgScopeConfig>(EASY_CONFIG_STR);
    //     assert!(config.is_ok())
    // }

    // #[test]
    // fn test_complex_config_de() {
    //     let _dummy_config = NgScopeConfig::default();
    //     let config = serde_libconfig::from_string::<NgScopeConfig>(COMPLEX_CONFIG_STR);
    //     assert!(config.is_ok())
    // }

    // #[test]
    // fn test_config_ser_de() {
    //     let dummy_config = NgScopeConfig::default();
    //     let config_str = serde_libconfig::to_string(&dummy_config);
    //     let config = serde_libconfig::from_string::<NgScopeConfig>(&config_str.unwrap()).unwrap();
    //     assert_eq!(config.nof_rf_dev, dummy_config.nof_rf_dev)
    // }
}
