use anyhow::Result;
use std::net::UdpSocket;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

mod cell_info;
mod ngscope;
mod util;
use cell_info::CellInfo;
use ngscope::config::NgScopeConfig;
use ngscope::types::Message;
use ngscope::{restart_ngscope, start_ngscope, stop_ngscope};

const MILESIGHT_BASE_ADDR: &str = "https://some.addr";

#[allow(dead_code)]
fn init_dci_server(local_addr: &str, server_addr: &str) -> Result<UdpSocket> {
    let socket = UdpSocket::bind(local_addr).unwrap();
    ngscope::ngscope_validate_server(&socket, server_addr).expect("server validation error");

    Ok(socket)
}

fn start_continuous_tracking() -> Result<()> {
    // Retrieve cell information
    // Write config
    // Start ng-scope process
    // loop:
    //   Retrieve cell (did it change?)
    //   Update config
    //   Restart ng-scope process
    //   -> implement hysterese: only restart if it has been running for a while.

    let sigint: Arc<AtomicBool> = util::prepare_sigint_notifier()?;
    let mut cell_info: CellInfo = CellInfo::from_milesight_router(MILESIGHT_BASE_ADDR)?;
    let mut ngscope_process: Child;
    let mut ngscope_config = NgScopeConfig::default();

    ngscope_config.rnti = 0xFFFF;
    ngscope_config.rf_config0.as_mut().unwrap().rf_freq = cell_info.down_frequency as u64;
    ngscope_process = start_ngscope(&ngscope_config)?;

    while !util::is_notifier(&sigint) {
        let latest_cell_info = CellInfo::from_milesight_router(MILESIGHT_BASE_ADDR)?;
        if latest_cell_info.cell_id != cell_info.cell_id {
            // TODO: Determine the RNIT using RNTI matching
            ngscope_config.rnti = 0xFFFF;
            ngscope_config.rf_config0.as_mut().unwrap().rf_freq =
                latest_cell_info.down_frequency as u64;
            ngscope_process = restart_ngscope(ngscope_process, &ngscope_config)?;
            cell_info = latest_cell_info.clone();
        }
        thread::sleep(Duration::from_secs(10));
    }
    stop_ngscope(ngscope_process)?;
    Ok(())
}

#[allow(dead_code)]
fn start_listen_for_ngscope_message() -> Result<()> {
    let local_addr = "0.0.0.0:8888";
    let server_addr = "0.0.0.0:6767";

    let socket = init_dci_server(local_addr, server_addr)?;

    println!("Successfully initialized Dci server");
    println!("Analyzing incoming messages..");

    loop {
        if let Ok(msg) = ngscope::ngscope_recv_single_message(&socket) {
            match msg {
                Message::Start => {}
                Message::Dci(ue_dci) => {
                    println!("{:?}", ue_dci)
                }
                Message::Config(cell_config) => {
                    println!("{:?}", cell_config)
                }
                Message::Exit => {
                    break;
                }
            }
        }
    }
    Ok(())
}

fn main() -> Result<()> {
    println!("Hello, world!");

    start_continuous_tracking()
}
