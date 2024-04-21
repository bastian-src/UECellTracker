use anyhow::Result;

use reqwest::get;
use std::collections::HashMap;

// Define the struct to hold cell information
#[derive(Debug, Clone)]
pub struct CellInfo {
    pub cell_id: u32,
    pub up_frequency: u32,
    pub down_frequency: u32,
}

pub fn frequencies_from_arfcn(_arfcn: u32) -> Result<(u32, u32)> {
    // TODO: Implement this properly
    Ok((123 as u32, 123 as u32))
}

impl CellInfo {
    // Function to create CellInfo from data fetched from Milesight router
    #[allow(dead_code)]
    #[tokio::main]
    pub async fn from_milesight_router(url: &str) -> Result<Self> {
        // Simulate fetching HTML response from a URL
        let resp = get(url)
            .await?
            .json::<HashMap<String, String>>()
            .await?;
        println!("{resp:#?}");

        // TODO: Parse data from request
        let cell_id = 12345;
        let (up_frequency, down_frequency) = frequencies_from_arfcn(1234)?;

        Ok(Self {
            cell_id,
            up_frequency,
            down_frequency,
        })
    }

    #[allow(dead_code)]
    pub async fn from_celldata_api(base_addr: &str) -> Result<Self> {
        let resp = get(String::new() + base_addr + "/api/cell")
            .await?
            .json::<HashMap<String, String>>()
            .await?;
        println!("{resp:#?}");

        // TODO: Use actual API parameters
        let cell_id = resp["cell_id"].parse::<u32>()?;
        let (up_frequency, down_frequency) =
            frequencies_from_arfcn(resp["up_frequency"].parse::<u32>()?)?;

        Ok(Self {
            cell_id,
            up_frequency,
            down_frequency,
        })
    }
}
