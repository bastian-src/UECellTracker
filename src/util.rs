#![allow(dead_code)]

use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use lazy_static::lazy_static;
use libc::{c_void, getsockopt, socklen_t, TCP_INFO};
use std::mem;

use crate::logger::log_info;

#[derive(Clone, Debug, Default)]
pub struct RingBuffer<T> {
    buffer: VecDeque<T>,
    size: usize,
}

#[derive(Clone, Debug, Default)]
pub struct CellRntiRingBuffer {
    cell_buffers: HashMap<u64, RingBuffer<u16>>,
    size: usize,
}

#[repr(C)]
#[derive(Debug, Default)]
pub struct StockTcpInfo {
    pub tcpi_state: u8,
    pub tcpi_ca_state: u8,
    pub tcpi_retransmits: u8,
    pub tcpi_probes: u8,
    pub tcpi_backoff: u8,
    pub tcpi_options: u8,
    pub tcpi_snd_wscale: u8,
    pub tcpi_rcv_wscale: u8,

    pub tcpi_rto: u32,
    pub tcpi_ato: u32,
    pub tcpi_snd_mss: u32,
    pub tcpi_rcv_mss: u32,

    pub tcpi_unacked: u32,
    pub tcpi_sacked: u32,
    pub tcpi_lost: u32,
    pub tcpi_retrans: u32,
    pub tcpi_fackets: u32,

    // Times
    pub tcpi_last_data_sent: u32,
    pub tcpi_last_ack_sent: u32,
    pub tcpi_last_data_recv: u32,
    pub tcpi_last_ack_recv: u32,

    // Metrics
    pub tcpi_pmtu: u32,
    pub tcpi_rcv_ssthresh: u32,
    pub tcpi_rtt: u32,
    pub tcpi_rttvar: u32,
    pub tcpi_snd_ssthresh: u32,
    pub tcpi_snd_cwnd: u32,
    pub tcpi_advmss: u32,
    pub tcpi_reordering: u32,

    pub tcpi_rcv_rtt: u32,
    pub tcpi_rcv_space: u32,

    pub tcpi_total_retrans: u32,
}

impl<T> RingBuffer<T>
where
    T: Eq + Hash + Clone,
{
    pub fn new(size: usize) -> Self {
        RingBuffer {
            buffer: VecDeque::with_capacity(size),
            size,
        }
    }

    pub fn add(&mut self, value: T) {
        if self.buffer.len() == self.size {
            self.buffer.pop_front(); // Remove the oldest value if the buffer is full
        }
        self.buffer.push_back(value); // Add the new value
    }

    pub fn most_frequent(&self) -> Option<T> {
        let mut frequency_map = std::collections::HashMap::new();
        for value in self.buffer.iter() {
            *frequency_map.entry(value).or_insert(0) += 1;
        }
        frequency_map
            .into_iter()
            .max_by_key(|&(_, count)| count)
            .map(|(value, _)| value.clone())
    }
}

impl CellRntiRingBuffer {
    pub fn new(size: usize) -> CellRntiRingBuffer {
        CellRntiRingBuffer {
            size,
            cell_buffers: HashMap::new(),
        }
    }

    pub fn update(&mut self, cell_rntis: &HashMap<u64, u16>) {
        for (&cell, &rnti) in cell_rntis.iter() {
            let cell_buffer = self
                .cell_buffers
                .entry(cell)
                .or_insert_with(|| RingBuffer::new(self.size));
            cell_buffer.add(rnti);
        }
    }

    pub fn most_frequent(&self) -> HashMap<u64, u16> {
        let mut cell_rntis: HashMap<u64, u16> = HashMap::new();
        for (&cell, cell_buffer) in self.cell_buffers.iter() {
            if let Some(most_frequent_rnti) = cell_buffer.most_frequent() {
                cell_rntis.insert(cell, most_frequent_rnti);
            }
        }
        cell_rntis
    }
}

pub fn prepare_sigint_notifier() -> Result<Arc<AtomicBool>> {
    let notifier = Arc::new(AtomicBool::new(false));
    let r = notifier.clone();
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    })
    .expect("Error setting Ctrl-C handler");
    Ok(notifier)
}

pub fn is_notifier(notifier: &Arc<AtomicBool>) -> bool {
    notifier.load(Ordering::SeqCst)
}

pub fn helper_json_pointer(
    nested_value: &serde_json::Value,
    pointer: &str,
) -> Result<serde_json::Value> {
    match nested_value.pointer(pointer) {
        Some(unnested_value) => Ok(unnested_value.clone()),
        _ => Err(anyhow!(
            "No value found by '{:?}' in {:#?}",
            pointer,
            nested_value
        )),
    }
}

pub fn determine_process_id() -> u64 {
    /* Taken from: https://github.com/alecmocatta/palaver/blob/cc955a9d56f187fe57a2d042e4ff4120d50dc3a7/src/thread.rs#L23 */
    match Into::<libc::pid_t>::into(nix::unistd::gettid()).try_into() {
        Ok(res) => res,
        Err(_) => std::process::id() as u64,
    }
}

pub fn print_dci(dci: crate::ngscope::types::NgScopeCellDci) {
    print_info(&format!(
        "DEBUG: {:?} | {:08?} | {:08?} | {:03?} | {:03?} | {:03?} | {:03?}",
        dci.nof_rnti,
        dci.total_dl_tbs_bit,
        dci.total_ul_tbs_bit,
        dci.total_dl_prb,
        dci.total_ul_prb,
        dci.total_dl_no_tbs_prb,
        dci.total_ul_no_tbs_prb,
    ));
}

pub fn print_info(s: &str) {
    let _ = log_info(s);
    // Log::print_info(s)
}

pub fn print_debug(s: &str) {
    if is_debug() {
        let _ = log_info(s);
        // Log::print_debug(s)
    }
}

lazy_static! {
    static ref IS_DEBUG: AtomicBool = AtomicBool::new(false);
}

pub fn set_debug(level: bool) {
    IS_DEBUG.store(level, Ordering::SeqCst);
}

pub fn is_debug() -> bool {
    IS_DEBUG.load(Ordering::SeqCst)
}

pub fn sockopt_get_tcp_info(socket_file_descriptor: i32) -> Result<StockTcpInfo> {
    let mut tcp_info: StockTcpInfo = StockTcpInfo::default();
    let mut tcp_info_len = mem::size_of::<StockTcpInfo>() as socklen_t;

    let ret = unsafe {
        getsockopt(
            socket_file_descriptor,
            libc::IPPROTO_TCP,
            TCP_INFO,
            &mut tcp_info as *mut _ as *mut c_void,
            &mut tcp_info_len,
        )
    };

    // Check if getsockopt was successful
    if ret != 0 {
        return Err(anyhow!("An error occured running libc::getsockopt"));
    }
    Ok(tcp_info)
}

pub fn init_heap_buffer(size: usize) -> Box<[u8]> {
    let mut vec: Vec<u8> = Vec::<u8>::with_capacity(size);
    /* Fill the vector with zeros */
    vec.resize_with(size, || 0);
    vec.into_boxed_slice()
}
