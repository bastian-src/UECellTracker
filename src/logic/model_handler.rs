use crate::cell_info::CellInfo;
use crate::ngscope::types::{NgScopeCellDci, NgScopeRntiDci};
use crate::parse::{Arguments, DynamicValue, FlattenedModelArgs};
use crate::util::{print_debug, print_info};
use std::collections::HashSet;
use std::sync::mpsc::{SyncSender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Result};
use bus::{Bus, BusReader};

use crate::logic::{
    check_not_stopped, wait_until_running, MainState, MessageCellInfo, MessageDci, MessageMetric,
    MessageRnti, ModelState, DEFAULT_WORKER_SLEEP_US,
};
use crate::util::determine_process_id;

pub const MAX_DCI_ARRAY_SIZE: usize = 10000;
pub const MAX_DCI_SLICE_SIZE: usize = 1000;
pub const MAX_DCI_SLICE_INDEX: usize = MAX_DCI_ARRAY_SIZE - MAX_DCI_SLICE_SIZE;
// Parameter gamma from [p. 456] PBE-CC: https://dl.acm.org/doi/abs/10.1145/3387514.3405880
pub const PHYSICAL_TO_TRANSPORT_OVERHEAD: f64 = 0.068;
pub const PHYSICAL_TO_TRANSPORT_FACTOR: f64 = 1.0 - PHYSICAL_TO_TRANSPORT_OVERHEAD;

struct DciRingBuffer {
    dci_array: Box<[NgScopeCellDci]>,
    dci_next: usize,
}

impl DciRingBuffer {
    fn new() -> DciRingBuffer {
        // Allocate the array on the heap directly
        let mut dci_vec = Vec::with_capacity(MAX_DCI_ARRAY_SIZE);
        dci_vec.resize_with(MAX_DCI_ARRAY_SIZE, NgScopeCellDci::default);

        DciRingBuffer {
            /* Allocate it on the HEAP */
            dci_array: dci_vec.into_boxed_slice(),
            dci_next: 0,
        }
    }

    fn push(&mut self, item: NgScopeCellDci) {
        if self.dci_next >= MAX_DCI_ARRAY_SIZE {
            // Copy last MAX_DCI_SLICE_SIZE items to the beginning
            let delta_index = self.dci_next - MAX_DCI_SLICE_SIZE;
            self.dci_array.copy_within(delta_index..self.dci_next, 0);
            self.dci_next = MAX_DCI_SLICE_SIZE;
        }
        self.dci_array[self.dci_next] = item;
        self.dci_next += 1;
    }

    fn slice(&self, slice_size: usize) -> &[NgScopeCellDci] {
        if slice_size > self.dci_next {
            return self.slice(self.dci_next);
        }
        let delta_index = self.dci_next - slice_size;
        &self.dci_array[delta_index..self.dci_next]
    }
}

use super::{MetricA, MetricTypes};

pub struct ModelHandlerArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_model_state: SyncSender<ModelState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub tx_metric: Bus<MessageMetric>,
}

struct RunArgs {
    pub app_args: Arguments,
    pub rx_app_state: BusReader<MainState>,
    pub tx_model_state: SyncSender<ModelState>,
    pub rx_cell_info: BusReader<MessageCellInfo>,
    pub rx_dci: BusReader<MessageDci>,
    pub rx_rnti: BusReader<MessageRnti>,
    pub tx_metric: Bus<MessageMetric>,
}

pub fn deploy_model_handler(args: ModelHandlerArgs) -> Result<JoinHandle<()>> {
    let mut run_args = RunArgs {
        app_args: args.app_args,
        rx_app_state: args.rx_app_state,
        tx_model_state: args.tx_model_state,
        rx_cell_info: args.rx_cell_info,
        rx_dci: args.rx_dci,
        rx_rnti: args.rx_rnti,
        tx_metric: args.tx_metric,
    };

    let thread = thread::spawn(move || {
        let _ = run(&mut run_args);
        finish(run_args);
    });
    Ok(thread)
}

fn send_final_state(tx_model_state: &SyncSender<ModelState>) -> Result<()> {
    Ok(tx_model_state.send(ModelState::Stopped)?)
}

fn wait_for_running(
    rx_app_state: &mut BusReader<MainState>,
    tx_model_state: &SyncSender<ModelState>,
) -> Result<()> {
    match wait_until_running(rx_app_state) {
        Ok(_) => Ok(()),
        _ => {
            send_final_state(tx_model_state)?;
            Err(anyhow!("[model] Main did not send 'Running' message"))
        }
    }
}

fn run(run_args: &mut RunArgs) -> Result<()> {
    let app_args = &run_args.app_args;
    let rx_app_state: &mut BusReader<MainState> = &mut run_args.rx_app_state;
    let tx_model_state: &mut SyncSender<ModelState> = &mut run_args.tx_model_state;
    let rx_cell_info: &mut BusReader<MessageCellInfo> = &mut run_args.rx_cell_info;
    let rx_dci: &mut BusReader<MessageDci> = &mut run_args.rx_dci;
    let rx_rnti: &mut BusReader<MessageRnti> = &mut run_args.rx_rnti;
    let tx_metric: &mut Bus<MessageMetric> = &mut run_args.tx_metric;

    tx_model_state.send(ModelState::Running)?;
    wait_for_running(rx_app_state, tx_model_state)?;
    print_info(&format!("[model]: \t\tPID {:?}", determine_process_id()));
    let sleep_duration = Duration::from_micros(DEFAULT_WORKER_SLEEP_US);

    let model_args = FlattenedModelArgs::from_unflattened(app_args.clone().model.unwrap())?;

    let mut last_metric_timestamp_us: u64 = chrono::Utc::now().timestamp_micros() as u64;
    let mut dci_buffer = DciRingBuffer::new();
    let mut last_rnti: Option<u16> = None;
    let mut last_cell_info: Option<CellInfo> = None;
    let last_rtt_us: Option<u64> = Some(40000); // TODO: Replace test-RTT with actual RTT and make
                                                // it mutable
    let mut metric_sending_interval_us: u64 = determine_sending_interval(&model_args, &last_rtt_us);
    let mut metric_smoothing_size_ms: u64 = determine_smoothing_size(&model_args, &last_rtt_us);

    loop {
        /* <precheck> */
        thread::sleep(sleep_duration);
        if check_not_stopped(rx_app_state).is_err() {
            break;
        }
        /* </precheck> */

        /* unpack dci, cell_info, rnti at every iteration to keep the queue "empty"! */
        // TODO: Unpack the RTT too
        match rx_dci.try_recv() {
            Ok(dci) => {
                dci_buffer.push(dci.ngscope_dci);
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        match rx_cell_info.try_recv() {
            Ok(cell_info) => last_cell_info = Some(cell_info.cell_info.clone()),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };
        match rx_rnti.try_recv() {
            Ok(rnti_msg) => {
                let rnti: u16 = rnti_msg.cell_rnti.values().copied().next().unwrap();
                last_rnti = Some(rnti);
                print_debug(&format!(
                    "DEBUG [model] new rnti {:#?}",
                    rnti_msg.cell_rnti.get(&0).unwrap()
                ));
            }
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        };

        if let (Some(rnti), Some(cell_info)) = (last_rnti, last_cell_info.clone()) {
            let delta_last_metric_sent_us =
                chrono::Utc::now().timestamp_micros() as u64 - last_metric_timestamp_us;
            if delta_last_metric_sent_us > metric_sending_interval_us {
                let buffer_slice_size: usize = metric_smoothing_size_ms as usize;
                let buffer_slice = dci_buffer.slice(buffer_slice_size);
                if !buffer_slice.is_empty() {
                    if let Ok((capacity_physical, phy_rate, phy_rate_flag, re_tx)) = calculate_capacity(rnti, &cell_info, buffer_slice) {
                        let capacity_transport = translate_physcial_to_transport_simple(capacity_physical);
                        print_debug(&format!("DEBUG [model] model:
                                             capacity: \t{:?}
                                             phy rate: \t{:?}
                                             phy flag: \t{:?}
                                             re-tx: \t{:?}", capacity_physical, phy_rate, phy_rate_flag, re_tx));
                        let now_us = chrono::Utc::now().timestamp_micros() as u64;
                        tx_metric.broadcast(MessageMetric {
                            metric: MetricTypes::A(MetricA {
                                timestamp_us: now_us,
                                fair_share_send_rate: capacity_transport,
                                latest_dci_timestamp_us: buffer_slice.first().unwrap().time_stamp,
                                oldest_dci_timestamp_us: buffer_slice.last().unwrap().time_stamp,
                                nof_dci: buffer_slice.len() as u16,
                                nof_re_tx: re_tx as u16,
                                flag_phy_rate_all_rnti: phy_rate_flag,
                                phy_rate, // TODO: Check whether it is reasonable to make this
                                          // u16/u32
                            }),
                        });
                    }
                    last_metric_timestamp_us = chrono::Utc::now().timestamp_micros() as u64;
                    metric_sending_interval_us =
                        determine_sending_interval(&model_args, &last_rtt_us);
                    metric_smoothing_size_ms = determine_smoothing_size(&model_args, &last_rtt_us);
                } else {
                    print_debug("DEBUG [model] skipping metric calculation, dci slice is 0");
                }
            }
            // TODO: Log the else case.
        }
    }

    Ok(())
}

fn finish(run_args: RunArgs) {
    let _ = send_final_state(&run_args.tx_model_state);
}

/*
 * PHY-Layer fair-share capacity in bit/ms in downlink
 * (capacity, bit/PRB ratio, re-transmissions)
 *
 * According to PBE-CC: https://dl.acm.org/doi/abs/10.1145/3387514.3405880
 * */
fn calculate_capacity(
    target_rnti: u16,
    cell_info: &CellInfo,
    dci_list: &[NgScopeCellDci]
) -> Result<(u64, u64, u8, u64)> {
    let nof_dci: u64 = dci_list.len() as u64;
    if nof_dci == 0  {
        return Err(anyhow!("Cannot calculate capacity with 0 DCI"));
    }
    let p_cell: f64 = cell_info.cells[0].nof_prb as f64 * nof_dci as f64;

    let nof_rnti: f64 = dci_list
        .iter()
        .flat_map(|dci| {
            dci.rnti_list
                .iter()
                .take(dci.nof_rnti as usize)
                .filter(|rnti_dci| rnti_dci.dl_prb > 0)
                .map(|rnti_dci| rnti_dci.rnti)
        })
        .collect::<HashSet<u16>>()
        .len() as f64;

    let p_alloc: u64 = dci_list
        .iter()
        .map(|dci| dci.total_dl_prb as u64)
        .sum();

    let tbs_alloc: u64 = dci_list
        .iter()
        .map(|dci| dci.total_dl_tbs )
        .sum();

    let target_rnti_dci_list: Vec<&NgScopeRntiDci> = dci_list
        .iter()
        .flat_map(|dci| {
            dci.rnti_list
                  .iter()
                  .take(dci.nof_rnti as usize)
                  .filter(|rnti_dci| rnti_dci.rnti == target_rnti)
                  .filter(|rnti_dci| rnti_dci.dl_prb > 0)
        })
        .collect::<Vec<&NgScopeRntiDci>>();

    let re_tx: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_reTx as u64 )
        .sum::<u64>();

    let tbs_alloc_rnti: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_tbs as u64 )
        .sum::<u64>();

    let p_alloc_rnti: u64 = target_rnti_dci_list
        .iter()
        .map(|target_rnti_dci| target_rnti_dci.dl_prb as u64 )
        .sum::<u64>();

    print_debug(&format!("DEBUG [model] parameters:
                         nof_dci: \t\t{:?}
                         p_cell: \t\t{:?}
                         nof_rnti: \t\t{:?}
                         p_alloc: \t\t{:?}
                         p_alloc_rnti: \t\t{:?}
                         tbs_alloc_rnti: \t{:?}
                         re_tx: \t\t{:?}",
                         nof_dci, p_cell, nof_rnti, p_alloc, p_alloc_rnti, tbs_alloc_rnti, re_tx));
    let mut flag_phy_rate_all_rnti: u8 = 0;
    // [bit/PRB]
    let r_w: u64 = if p_alloc_rnti == 0 {
        flag_phy_rate_all_rnti = 1;
        8 * tbs_alloc / p_alloc
    } else {
        8 * tbs_alloc_rnti / p_alloc_rnti
    };
    let p_idle: f64 = p_cell - p_alloc as f64; 
    let c_p: u64 = ((r_w as f64 * (p_alloc_rnti as f64 + (p_idle / nof_rnti))) / nof_dci as f64) as u64;
    Ok((c_p, r_w, flag_phy_rate_all_rnti, re_tx))
}

fn translate_physcial_to_transport_simple(c_physical: u64) -> u64 {
    (c_physical as f64 * PHYSICAL_TO_TRANSPORT_FACTOR) as u64
}

fn determine_sending_interval(model_args: &FlattenedModelArgs, last_rtt_us: &Option<u64>) -> u64 {
    match model_args.model_send_metric_interval_type {
        DynamicValue::FixedMs => model_args.model_send_metric_interval_value as u64,
        DynamicValue::RttFactor => {
            (last_rtt_us.unwrap() as f64 * model_args.model_send_metric_interval_value) as u64
        }
    }
}

fn determine_smoothing_size(model_args: &FlattenedModelArgs, last_rtt_us: &Option<u64>) -> u64 {
    let unbound_slice = match model_args.model_metric_smoothing_size_type {
        DynamicValue::FixedMs => model_args.model_metric_smoothing_size_value as u64,
        DynamicValue::RttFactor => {
            (last_rtt_us.unwrap() as f64 * model_args.model_metric_smoothing_size_value / 1000.0) as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{ngscope::types::{NgScopeRntiDci, NGSCOPE_MAX_NOF_RNTI}, cell_info::SingleCell};
    use super::*;

    fn dummy_rnti_dci(nof_rnti: u8) -> [NgScopeRntiDci; NGSCOPE_MAX_NOF_RNTI] {
        let mut rnti_list = [NgScopeRntiDci::default(); NGSCOPE_MAX_NOF_RNTI];
        for i in 0..nof_rnti {
            rnti_list[0].dl_tbs = 1024;
            rnti_list[0].dl_prb = 2 + i;
            rnti_list[0].rnti = 123 + i as u16;
        }

        rnti_list
    }

    fn dummy_dci_slice() -> Vec<NgScopeCellDci> {
        vec![
            NgScopeCellDci {
                nof_rnti: 2,
                total_dl_prb: 5,
                rnti_list: dummy_rnti_dci(2),
                ..Default::default()
            },
            NgScopeCellDci {
                nof_rnti: 1,
                total_dl_prb: 2,
                rnti_list: dummy_rnti_dci(1),
                ..Default::default()
            },
            NgScopeCellDci {
                nof_rnti: 4,
                total_dl_prb: 14,
                rnti_list: dummy_rnti_dci(4),
                ..Default::default()
            },
        ]
    }

    #[test]
    fn test_capacity() -> Result<()> {
        let dummy_rnti: u16 = 123;
        let dummy_cell_info = CellInfo {
            cells: vec![
                SingleCell {
                    nof_prb: 100,
                    ..Default::default()
                }
            ]
        };
        let actual_capacity = calculate_capacity(dummy_rnti, &dummy_cell_info, &dummy_dci_slice())?;
        assert_eq!(actual_capacity, (129706, 512 * 8, 0, 0));
        Ok(())
    }
}
