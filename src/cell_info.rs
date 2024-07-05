use std::time::Duration;

use anyhow::{anyhow, Result};

use regex::Regex;
use reqwest::header::{HeaderMap, HeaderName};
use serde_derive::Deserialize;

use crate::util::helper_json_pointer;

pub const REQUEST_TIMEOUT_MS: u64 = 2000;

#[derive(Debug, Clone, PartialEq, Default)]
#[allow(clippy::upper_case_acronyms)]
pub enum CellularType {
    #[default]
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
    pub cells: Vec<SingleCell>,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct SingleCell {
    pub cell_id: u64,
    pub cell_type: CellularType,
    pub nof_prb: u16,
    pub frequency: u64,
    pub rssi: f64,
    pub rsrp: f64,
    pub rsrq: f64,
    pub dl_est: Option<f64>,
    pub ul_est: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
#[allow(non_snake_case, dead_code)]
pub struct CellData {
    pub nodeB: Option<u64>,
    pub cid: Option<u64>,
    pub pci: Option<u64>,
    pub r#type: String,
    pub arfcn: u64,
    pub band: String,
    pub rssi: f64,
    pub rsrp: f64,
    pub rsrq: f64,
    pub estimatedDownBandwidth: Option<f64>,
    pub estimatedUpBandwidth: Option<f64>,
}

pub fn arfcn_to_frequency(arfcn: u64, cell_type: &CellularType) -> Result<u64> {
    match *cell_type {
        CellularType::LTE => {
            if (0..=599).contains(&arfcn) {
                // Band 1
                Ok(2110000000 + 100000 * arfcn)
            } else if (600..=1199).contains(&arfcn) {
                // Band 2
                Ok(1930000000 + 100000 * (arfcn - 600))
            } else if (1200..=1949).contains(&arfcn) {
                // Band 3
                Ok(1805000000 + 100000 * (arfcn - 1200))
            } else if (1950..=2399).contains(&arfcn) {
                // Band 4
                Ok(2110000000 + 100000 * (arfcn - 1949))
            } else if (2400..=2649).contains(&arfcn) {
                // Band 5
                Ok(869000000 + 100000 * (arfcn - 2400))
            } else if (2750..=3449).contains(&arfcn) {
                // Band 7
                Ok(2620000000 + 100000 * (arfcn - 2750))
            } else if (3450..=3799).contains(&arfcn) {
                // Band 8
                Ok(925000000 + 100000 * (arfcn - 3450))
            } else if (3800..=4149).contains(&arfcn) {
                // Band 9
                Ok(1844900000 + 100000 * (arfcn - 3800))
            } else if (4150..=4749).contains(&arfcn) {
                // Band 10
                Ok(2110000000 + 100000 * (arfcn - 4150))
            } else if (4750..=4949).contains(&arfcn) {
                // Band 11
                Ok(1475900000 + 100000 * (arfcn - 4750))
            } else if (5010..=5179).contains(&arfcn) {
                // Band 12
                Ok(729000000 + 100000 * (arfcn - 5010))
            } else if (5180..=5279).contains(&arfcn) {
                // Band 13
                Ok(746000000 + 100000 * (arfcn - 5180))
            } else if (5280..=5379).contains(&arfcn) {
                // Band 14
                Ok(758000000 + 100000 * (arfcn - 5280))
            } else if (5730..=5849).contains(&arfcn) {
                // Band 17
                Ok(734000000 + 100000 * (arfcn - 5730))
            } else if (5850..=5999).contains(&arfcn) {
                // Band 18
                Ok(860000000 + 100000 * (arfcn - 5850))
            } else if (6000..=6149).contains(&arfcn) {
                // Band 19
                Ok(875000000 + 100000 * (arfcn - 6000))
            } else if (6150..=6449).contains(&arfcn) {
                // Band 20
                Ok(791000000 + 100000 * (arfcn - 6150))
            } else if (6450..=6599).contains(&arfcn) {
                // Band 21
                Ok(1495900000 + 100000 * (arfcn - 6450))
            } else if (6600..=7399).contains(&arfcn) {
                // Band 22
                Ok(3510000000 + 100000 * (arfcn - 6600))
            } else if (7700..=8039).contains(&arfcn) {
                // Band 24
                Ok(1525000000 + 100000 * (arfcn - 7700))
            } else if (8040..=8689).contains(&arfcn) {
                // Band 25
                Ok(1930000000 + 100000 * (arfcn - 8040))
            } else if (8690..=9039).contains(&arfcn) {
                // Band 26
                Ok(859000000 + 100000 * (arfcn - 8690))
            } else if (9040..=9209).contains(&arfcn) {
                // Band 27
                Ok(852000000 + 100000 * (arfcn - 9040))
            } else if (9210..=9659).contains(&arfcn) {
                // Band 28
                Ok(758000000 + 100000 * (arfcn - 9210))
            } else if (9660..=9769).contains(&arfcn) {
                // Band 29
                Ok(728000000 + 100000 * (arfcn - 9660))
            } else if (9770..=9869).contains(&arfcn) {
                // Band 30
                Ok(2350000000 + 100000 * (arfcn - 9770))
            } else if (9870..=9919).contains(&arfcn) {
                // Band 31
                Ok(462500000 + 100000 * (arfcn - 9870))
            } else if (9919..=10359).contains(&arfcn) {
                // Band 32
                Ok(1492000000 + 100000 * (arfcn - 9919))
            } else if (131072..=131971).contains(&arfcn) {
                // Band 65
                Ok(2110000000 + 100000 * (arfcn - 131072))
            } else if (131972..=132671).contains(&arfcn) {
                // Band 66
                Ok(2110000000 + 100000 * (arfcn - 131972))
            } else if (132672..=132971).contains(&arfcn) {
                // Band 68
                Ok(753000000 + 100000 * (arfcn - 132672))
            } else if (132972..=133121).contains(&arfcn) {
                // Band 70
                Ok(1995000000 + 100000 * (arfcn - 132972))
            } else if (133122..=133471).contains(&arfcn) {
                // Band 71
                Ok(617000000 + 100000 * (arfcn - 133122))
            } else {
                Err(anyhow!("ARFCN out of range"))
            }
        }
        CellularType::NR => {
            let (delta_f_global, f_ref_offs, n_ref_offs) = match arfcn {
                0..=599999 => (5, 0, 0),
                600000..=2016666 => (15, 3000000, 600000),
                _ => (60, 24250080, 2016667),
            };
            // let n_ref = arfcn;
            let freq = (f_ref_offs + (delta_f_global * (arfcn - n_ref_offs))) * 1000;
            Ok(freq)
        }
    }
}

impl CellData {
    /// Returns the first non-`None` identifier among `cid`, `pci`, and `nodeB`.
    /// Returns `0` as fallback if all are `None`.
    ///
    /// # Examples
    ///
    /// ```
    /// let cell_data = CellData { cid: None, pci: Some(456), nodeB: None };
    /// assert_eq!(cell_data.safe_id(), 456);
    /// ```
    pub fn safe_id(&self) -> u64 {
        self.cid.or(self.pci).or(self.nodeB).unwrap_or(0)
    }
}

impl CellInfo {
    // Do not rely on cell_id, just check if frequency and cell_type are the same
    pub fn equal_content(info_a: &CellInfo, info_b: &CellInfo) -> bool {
        let a_cells = &info_a.cells;
        let b_cells = &info_b.cells;

        if a_cells.len() != b_cells.len() {
            return false;
        }
        a_cells.iter().all(|a_cell| {
            b_cells.iter().any(|b_cell| {
                a_cell.frequency == b_cell.frequency && a_cell.cell_type == b_cell.cell_type
            })
        })
    }
}

async fn cgi_get_token(base_addr: &str, user: &str, auth: &str) -> Result<HeaderMap> {
    let url = format!("http://{}/cgi", base_addr);
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
        .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
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
    let url = format!("http://{}/cgi", base_addr);
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
        .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .header("Content-Type", "application/json")
        .body(payload)
        .send()
        .await?;
    let body = resp.text().await?;
    let response_value: serde_json::Value = serde_json::from_str(&body)?;
    Ok(response_value)
}

async fn devpub_get_cell(base_addr: &str) -> Result<String> {
    let url = format!("http://{}:7353/api/v1/celldata/connected/all", base_addr);
    let resp = reqwest::Client::new()
        .get(&url)
        .timeout(Duration::from_millis(REQUEST_TIMEOUT_MS))
        .header("Accept", "application/json, text/javascript, */*; q=0.01")
        .send()
        .await?;
    let body = resp.text().await?;
    Ok(body)
}

impl CellInfo {
    #[allow(dead_code)]
    #[tokio::main]
    pub async fn from_milesight_router(base_addr: &str, user: &str, auth: &str) -> Result<Self> {
        let token_headermap = cgi_get_token(base_addr, user, auth).await?;
        let response_json = cgi_get_cell(base_addr, &token_headermap).await?;
        let cell_info = Self::from_cgi_response(&response_json)?;
        Ok(cell_info)
    }

    #[allow(dead_code)]
    #[tokio::main]
    pub async fn from_devicepublisher(base_addr: &str) -> Result<Self> {
        let response_json = devpub_get_cell(base_addr).await?;
        let cell_data = serde_json::from_str::<Vec<CellData>>(&response_json)?;
        let cell_info = Self::from_devpub_celldata(cell_data)?;
        Ok(cell_info)
    }

    /* -------------------------- */
    /*      Helper functions      */
    /* -------------------------- */

    pub fn from_cgi_response(response_value: &serde_json::Value) -> Result<CellInfo> {
        let mut single_cell: SingleCell = SingleCell {
            cell_id: 0,
            cell_type: CellularType::LTE,
            nof_prb: 100,
            frequency: 0,
            rssi: 0.0,
            rsrp: 0.0,
            rsrq: 0.0,
            dl_est: None,
            ul_est: None,
        };

        single_cell.cell_id = cgi_response_extract_cell_id(response_value)?;
        single_cell.cell_type = cgi_response_extract_cell_type(response_value)?;
        single_cell.frequency =
            cgi_response_extract_frequency(response_value, &single_cell.cell_type)?;
        single_cell.rssi = cgi_response_extract_rssi(response_value)?;
        single_cell.rsrq = cgi_response_extract_rsrp(response_value)?;
        single_cell.rsrp = cgi_response_extract_rsrq(response_value)?;
        single_cell.nof_prb = prb_from_cell_id(single_cell.cell_id);
        // TODO: Evaluate: Add estimated bandwidth?
        Ok(CellInfo {
            cells: [single_cell].to_vec(),
        })
    }

    pub fn from_devpub_celldata(cell_data: Vec<CellData>) -> Result<CellInfo> {
        let mut cell_info = CellInfo { cells: vec![] };
        for cell in cell_data.iter() {
            let mut single_cell = SingleCell {
                cell_id: cell.safe_id(),
                cell_type: CellularType::from_str(&cell.r#type)?,
                frequency: 0,
                nof_prb: 100,
                rssi: cell.rssi,
                rsrp: cell.rsrp,
                rsrq: cell.rsrq,
                dl_est: cell.estimatedDownBandwidth,
                ul_est: cell.estimatedUpBandwidth,
            };
            single_cell.frequency = arfcn_to_frequency(cell.arfcn, &single_cell.cell_type)?;
            single_cell.nof_prb = prb_from_cell_id(single_cell.cell_id);
            cell_info.cells.push(single_cell);
        }

        Ok(cell_info)
    }
}

// Quick fix for setting the nof PRB.
fn prb_from_cell_id(cell_id: u64) -> u16 {
    match cell_id {
        /* O2 */
        21 => 50,
        41 => 100,
        51 => 100,
        61 => 100,
        63 => 100,
        /* Telekom */
        6 => 50,
        7 => 100,
        8 => 100,
        _ => 100,
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
) -> Result<u64> {
    let lte_pointer = "/result/0/get/0/value/more/earfcn";
    let nr_pointer = "/result/0/get/0/value/more/nrarfcn";
    if let Some(arfcn_str) = helper_json_pointer(response, lte_pointer)?.as_str() {
        return arfcn_to_frequency(arfcn_str.parse::<u64>()?, cell_type);
    } else if let Some(arfcn_str) = helper_json_pointer(response, nr_pointer)?.as_str() {
        return arfcn_to_frequency(arfcn_str.parse::<u64>()?, cell_type);
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

    const DUMMY_DEVICEPUBLISHER_RESPONSE: &str = r#"[
    {
        "nodeB": 20321,
        "cid": null,
        "pci": null,
        "type": "LTE",
        "arfcn": 1801,
        "band": "1800",
        "rssi": -89,
        "rsrq": -13.0,
        "rsrp": -120.0,
        "estimatedDownBandwidth": 18245,
        "estimatedUpBandwidth": 9064
    }
]"#;

    #[test]
    fn test_arfcn_to_frequency_lte() -> Result<()> {
        let af_map_lte = [
            (2750, 2620000000), // Band 7
            (1710, 1856000000), // Band 3
            (4400, 2135000000), // Band 10
            (5300, 760000000),  // Band 5
            (6000, 875000000),  // Band 19
            (6300, 806000000),  // Band 20
            (300, 2140000000),  // Band 1
        ];
        let cell_type = CellularType::LTE;
        assert_eq!(
            arfcn_to_frequency(af_map_lte[0].0, &cell_type)?,
            af_map_lte[0].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[1].0, &cell_type)?,
            af_map_lte[1].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[2].0, &cell_type)?,
            af_map_lte[2].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[3].0, &cell_type)?,
            af_map_lte[3].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[4].0, &cell_type)?,
            af_map_lte[4].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[5].0, &cell_type)?,
            af_map_lte[5].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_lte[6].0, &cell_type)?,
            af_map_lte[6].1
        );
        Ok(())
    }

    #[test]
    fn test_arfcn_to_frequency_nr() -> Result<()> {
        let af_map_nr = [
            (151600, 758000000),    // Band n28
            (361000, 1805000000),   // Band n3
            (422000, 2110000000),   // Band n1
            (620000, 3300000000),   // Band n78
            (2016667, 24250080000), // Band n258
        ];
        let cell_type = CellularType::NR;
        assert_eq!(
            arfcn_to_frequency(af_map_nr[0].0, &cell_type)?,
            af_map_nr[0].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_nr[1].0, &cell_type)?,
            af_map_nr[1].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_nr[2].0, &cell_type)?,
            af_map_nr[2].1
        );
        assert_eq!(
            arfcn_to_frequency(af_map_nr[3].0, &cell_type)?,
            af_map_nr[3].1
        );
        Ok(())
    }

    /* -------------------------- */
    /*     Milesight cgi Tests    */
    /* -------------------------- */

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
        assert_eq!(frequency, 1815000000);
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

    /* -------------------------- */
    /*    DevicePublisher tests   */
    /* -------------------------- */

    #[test]
    fn test_devpub_from_celldata() -> Result<()> {
        let cell_data = serde_json::from_str::<Vec<CellData>>(DUMMY_DEVICEPUBLISHER_RESPONSE)?;
        let cell_info = CellInfo::from_devpub_celldata(cell_data)?;
        assert_eq!(cell_info.cells.first().unwrap().cell_id, 20321);
        assert_eq!(
            cell_info.cells.first().unwrap().cell_type,
            CellularType::LTE
        );
        assert_eq!(cell_info.cells.first().unwrap().rssi, -89.0);
        assert_eq!(cell_info.cells.first().unwrap().rsrp, -120.0);
        assert_eq!(cell_info.cells.first().unwrap().rsrq, -13.0);
        assert_eq!(cell_info.cells.first().unwrap().frequency, 1865100000);
        Ok(())
    }
}
