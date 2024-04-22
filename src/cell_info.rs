use anyhow::{anyhow, Result};

use regex::Regex;
use reqwest::get;
use reqwest::header::{HeaderMap, HeaderName};
use std::collections::HashMap;

use crate::helper::helper_json_pointer;

// TODO: CellInfo should be able to hold several cell
// Define the struct to hold cell information

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
pub enum CellularType {
    LTE,
    NR,
}

impl CellularType {
    fn from_str(cell_str: &str) -> Result<CellularType> {
        match cell_str {
            "LTE" | "4G" | "4G LTE" => Ok(CellularType::LTE),
            "NR" | "5G" | "5G NR" => Ok(CellularType::NR),
            _ => Err(anyhow!("Cannot convert string to CellularType: {cell_str}")),
        }
    }
}

#[derive(Debug, Clone)]
pub struct CellInfo {
    pub cell_id: u64,
    pub cell_type: CellularType,
    pub frequency: u32,
    pub rssi: f64,
    pub rsrp: f64,
    pub rsrq: f64,
    pub dl_est: Option<f64>,
    pub ul_est: Option<f64>,
}

pub fn arfcn_to_frequency(_arfcn: u32, _cell_type: &CellularType) -> Result<u32> {
    Ok(123_u32)
}

async fn cgi_get_token(base_addr: &str, user: &str, auth: &str) -> Result<HeaderMap> {
    let url = format!("{}/cgi", base_addr);
    let payload = format!(
        "{{
            \"id\":\"1\",\
            \"execute\":1,\
            \"core\":\"user\",\
            \"function\":\"login\",\
            \"values\":[{{\"username\":\"{}\",\"password\":\"{}\"}}]
          }}",
        user, auth
    );
    let resp = reqwest::Client::new()
        .get(&url)
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .await?;
    let set_cookie_str: String;
    if resp.headers().contains_key("Set-Cookie") || resp.headers().contains_key("set-cookie") {
        set_cookie_str = "Set-Cookie".to_string();
    } else if resp.headers().contains_key("set-cookie") {
        set_cookie_str = "set-cookie".to_string();
    } else {
        return Err(anyhow!("Could not extract token from response: {resp:#?}"));
    }
    let mut header_map = HeaderMap::new();
    let set_cookie = HeaderName::from_static("set-cookie");
    for content in resp.headers().get_all(set_cookie_str).iter() {
        header_map.append(set_cookie.clone(), content.clone());
    }
    Ok(header_map)
}

async fn cgi_get_cell(base_addr: &str, header_map: &HeaderMap) -> Result<serde_json::Value> {
    let url = format!("{}/cgi", base_addr);
    let payload = "{\"id\":\"1\",\
                            \"execute\":1,\
                            \"core\":\"user\",\
                            \"function\":\"get\",\
                            \"values\":[{\"base\":\"yruo_celluar\"}]}"
        .to_string();
    let resp = reqwest::Client::builder()
        .default_headers(header_map.clone())
        .build()?
        .get(&url)
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .await?;
    let body = resp.text().await?;
    let response_value: serde_json::Value = serde_json::from_str(&body)?;
    Ok(response_value)
}

impl CellInfo {
    #[allow(dead_code)]
    #[tokio::main]
    pub async fn from_milesight_router(base_addr: &str, user: &str, auth: &str) -> Result<Self> {
        // Simulate fetching HTML response from a URL
        let token_headermap = cgi_get_token(base_addr, user, auth).await?;
        println!("{token_headermap:#?}");
        let response_json = cgi_get_cell(base_addr, &token_headermap).await?;
        let cell_info = CellInfo::from_cgi_response(&response_json)?;
        Ok(cell_info)
    }

    #[allow(dead_code)]
    pub async fn from_celldata_api(base_addr: &str) -> Result<Self> {
        let resp = get(String::new() + base_addr + "/api/cell")
            .await?
            .json::<HashMap<String, String>>()
            .await?;
        println!("{resp:#?}");
        // TODO: Use actual API parameters
        todo!()
    }

    /* -------------------------- */
    /*      Helper functions      */
    /* -------------------------- */

    pub fn from_cgi_response(response_value: &serde_json::Value) -> Result<CellInfo> {
        let mut cell_info: CellInfo = CellInfo {
            cell_id: 0,
            cell_type: CellularType::LTE,
            frequency: 0,
            rssi: 0.0,
            rsrp: 0.0,
            rsrq: 0.0,
            dl_est: None,
            ul_est: None,
        };

        cell_info.cell_id = cgi_response_extract_cell_id(response_value)?;
        cell_info.cell_type = cgi_response_extract_cell_type(response_value)?;
        cell_info.frequency = cgi_response_extract_frequency(response_value, &cell_info.cell_type)?;
        cell_info.rssi = cgi_response_extract_rssi(response_value)?;
        cell_info.rsrq = cgi_response_extract_rsrp(response_value)?;
        cell_info.rsrp = cgi_response_extract_rsrq(response_value)?;
        // TODO: Evaluate: Add estimated bandwidth?
        Ok(cell_info)
    }
}

/* --------------------------- */
/*    Milesight cgi helpers    */
/* --------------------------- */

fn cgi_response_extract_cell_id(response: &serde_json::Value) -> Result<u64> {
    let pointer = "/result/0/get/0/value/modem/cellid";
    match helper_json_pointer(response, pointer)?.as_str() {
        Some(cell_id_str) => {
            let cell_id = u64::from_str_radix(cell_id_str, 16)?;
            Ok(cell_id)
        }
        _ => Err(anyhow!(
            "Could not extract cell id from response: {:#?}\n
            path: {:?}",
            response,
            pointer
        )),
    }
}

fn cgi_response_extract_cell_type(response: &serde_json::Value) -> Result<CellularType> {
    let pointer = "/result/0/get/0/value/modem/net_type";
    match helper_json_pointer(response, pointer)?.as_str() {
        Some(cell_type) => CellularType::from_str(cell_type),
        _ => Err(anyhow!(
            "Could not extract cell type from response: {:#?}\n
            path: {:?}",
            response,
            pointer
        )),
    }
}

fn cgi_response_extract_frequency(
    response: &serde_json::Value,
    cell_type: &CellularType,
) -> Result<u32> {
    let lte_pointer = "/result/0/get/0/value/more/earfcn";
    let nr_pointer = "/result/0/get/0/value/more/nrarfcn";
    if let Some(arfcn_str) = helper_json_pointer(response, lte_pointer)?.as_str() {
        return arfcn_to_frequency(arfcn_str.parse::<u32>()?, cell_type);
    } else if let Some(arfcn_str) = helper_json_pointer(response, nr_pointer)?.as_str() {
        return arfcn_to_frequency(arfcn_str.parse::<u32>()?, cell_type);
    }
    Err(anyhow!(
        "Could not extract ARFCN from response: {:#?}\n
        path LTE: {:?}\n
        path NR: {:?}",
        response,
        lte_pointer,
        nr_pointer
    ))
}

fn cgi_response_extract_rssi(response: &serde_json::Value) -> Result<f64> {
    let pointer = "/result/0/get/0/value/modem/signal";
    let re = Regex::new(r"\((-?\d+)dBm\)").unwrap();

    if let Some(rssi_str) = helper_json_pointer(response, pointer)?.as_str() {
        if let Some(captures) = re.captures(rssi_str) {
            if let Some(dpm_str) = captures.get(1) {
                return Ok(dpm_str.as_str().parse::<f64>()?);
            }
        }
    }
    Err(anyhow!(
        "Could not extract RSSI from response: {:#?}\n
        path: {:?}\n
        regex: {:?}",
        response,
        pointer,
        re.as_str()
    ))
}

fn cgi_response_extract_rsrp(response: &serde_json::Value) -> Result<f64> {
    let pointer = "/result/0/get/0/value/more/rsrp";
    match helper_json_pointer(response, pointer)?.as_str() {
        Some(rsrp_str) => {
            let end = rsrp_str.len() - 3;
            Ok(rsrp_str[..end].trim().parse::<f64>()?)
        }
        _ => Err(anyhow!(
            "Could not extract RSRP from response: {:#?}\n
            path: {:?}",
            response,
            pointer
        )),
    }
}

fn cgi_response_extract_rsrq(response: &serde_json::Value) -> Result<f64> {
    let pointer = "/result/0/get/0/value/more/rsrq";
    match helper_json_pointer(response, pointer)?.as_str() {
        Some(rsrp_str) => {
            let end = rsrp_str.len() - 2;
            Ok(rsrp_str[..end].trim().parse::<f64>()?)
        }
        _ => Err(anyhow!(
            "Could not extract RSRQ from response: {:#?}\n
            path: {:?}",
            response,
            pointer
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn dummy_response() -> serde_json::Value {
        json!({
            "result": [
                {
                    "get": [
                        {
                            "value": {
                                "modem": {
                                    "signal": "31asu (-51dBm)",
                                    "net_type": "LTE",
                                    "cellid": "1C17302",
                                },
                                "more": {
                                    "rsrp": "-77dBm",
                                    "rsrq": "-8dB",
                                    "earfcn": "1300",
                                }
                            }
                        }
                    ]
                }
            ]
        })
    }

    #[test]
    fn test_cgi_response_extract_cell_id() -> Result<()> {
        let cell_id = cgi_response_extract_cell_id(&dummy_response())?;
        assert_eq!(cell_id, 0x1C17302);
        Ok(())
    }

    #[test]
    fn test_cgi_response_extract_cell_type() -> Result<()> {
        let cell_type = cgi_response_extract_cell_type(&dummy_response())?;
        assert_eq!(cell_type, CellularType::LTE);
        Ok(())
    }

    #[test]
    fn test_cgi_response_extract_frequency() -> Result<()> {
        let frequency = cgi_response_extract_frequency(&dummy_response(), &CellularType::LTE)?;
        assert_eq!(frequency, 123_u32);
        Ok(())
    }

    #[test]
    fn test_cgi_response_extract_rssi() -> Result<()> {
        let rsrp = cgi_response_extract_rssi(&dummy_response())?;
        assert_eq!(rsrp, -51.0);
        Ok(())
    }

    #[test]
    fn test_cgi_response_extract_rsrp() -> Result<()> {
        let rsrp = cgi_response_extract_rsrp(&dummy_response())?;
        assert_eq!(rsrp, -77.0);
        Ok(())
    }

    #[test]
    fn test_cgi_response_extract_rsrq() -> Result<()> {
        let rsrp = cgi_response_extract_rsrq(&dummy_response())?;
        assert_eq!(rsrp, -8.0);
        Ok(())
    }
}
