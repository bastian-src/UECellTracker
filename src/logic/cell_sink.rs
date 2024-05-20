use crate::util::{print_debug, print_info};
use std::sync::mpsc::{SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use bus::BusReader;

use crate::logic::{
    check_not_stopped, wait_until_running, MainState, MessageCellInfo, MessageDci, MessageRnti,
    SinkState, DEFAULT_WORKER_SLEEP_US,
};
use crate::util::determine_process_id;

pub struct CellSinkArgs {
    pub rx_app_state: BusReader<MainState>,
    pub tx_sink_state: SyncSender<SinkState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
}

pub fn deploy_cell_sink(mut args: CellSinkArgs) -> Result<JoinHandle<()>> {
    let thread = thread::spawn(move || {
        let _ = run(
            args.rx_app_state,
            args.tx_sink_state,
            &mut args.rx_cell_info,
            &mut args.rx_dci,
            &mut args.rx_rnti,
        );
    });
    Ok(thread)
}

fn send_final_state(tx_sink_state: &SyncSender<SinkState>) -> Result<()> {
    Ok(tx_sink_state.send(SinkState::Stopped)?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<MainState>,
    tx_sink_state: &SyncSender<SinkState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_sink_state)?;
            Err(anyhow!("[sink] Main did not send 'Running' message"))
        }
    }
}

fn run(
    mut rx_app_state: BusReader<MainState>,
    tx_sink_state: SyncSender<SinkState>,
    rx_cell_info: &mut BusReader<MessageCellInfo>,
    rx_dci: &mut BusReader<MessageDci>,
    rx_rnti: &mut BusReader<MessageRnti>,
) -> Result<()> {
    tx_sink_state.send(SinkState::Running)?;
    wait_for_running(&mut rx_app_state, &tx_sink_state)?;
    print_info(&format!("[sink]: \t\tPID {:?}", determine_process_id()));
    let sleep_duration = Duration::from_micros(DEFAULT_WORKER_SLEEP_US);

    loop {
        /* <precheck> */
        thread::sleep(sleep_duration);
        if check_not_stopped(&mut rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        /* unpack dci, cell_info, rnti at every iteration to keep the queue "empty"! */
        let _new_dci = match rx_dci.try_recv() {
            Ok(dci) => Some(dci),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => break,
        };
        let _new_cell_info = match rx_cell_info.try_recv() {
            Ok(cell_info) => Some(cell_info),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => break,
        };
        let new_rnti = match rx_rnti.try_recv() {
            Ok(rnti) => Some(rnti),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => break,
        };

        if let Some(rnti_msg) = new_rnti {
            if !rnti_msg.cell_rnti.is_empty() {
                print_debug(&format!(
                    "DEBUG [sink] new rnti {:#?}",
                    rnti_msg.cell_rnti.get(&0).unwrap()
                ));
            }
        }

        // TODO: Consume rx_dci, rx_cell_info, and rx_rnti
        // TODO: -> Send combined message to some remote
    }

    send_final_state(&tx_sink_state)?;
    Ok(())
}
