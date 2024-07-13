#![allow(dead_code)]

use anyhow::{anyhow, Result};
use casual_logger::{Level, Log};
use lazy_static::lazy_static;
use std::collections::{HashMap, VecDeque};
use std::fs::{File, OpenOptions};
use std::hash::Hash;
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::logger::log_info;
use crate::logic::rnti_matcher::TrafficCollection;

pub const MATCHING_LOG_FILE_PREFIX: &str = "./.logs/rnti_matching_pattern_";
pub const MATCHING_LOG_FILE_SUFFIX: &str = ".jsonl";

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
        "DEBUG: {:?} | {:03?} | {:03?} | {:08?} | {:08?} | {:03?} | {:03?}",
        dci.nof_rnti,
        dci.total_dl_prb,
        dci.total_ul_prb,
        dci.total_dl_tbs,
        dci.total_ul_tbs,
        dci.total_dl_reTx,
        dci.total_ul_reTx
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

pub trait LogExt {
    fn print_info(s: &str);
    fn print_debug(s: &str);
}
impl LogExt for Log {
    /// Info level logging and add print to stdout.
    fn print_info(s: &str) {
        if Log::enabled(Level::Info) {
            println!("{}", s);
        }
        Log::infoln(s);
    }

    fn print_debug(s: &str) {
        if Log::enabled(Level::Debug) {
            println!("{}", s);
        }
        Log::debugln(s);
    }
}

pub fn log_rnti_matching_traffic(traffic_collection: &TrafficCollection) -> Result<()> {
    let matching_log_file_path = &format!(
        "{}{:?}{}",
        MATCHING_LOG_FILE_PREFIX,
        traffic_collection.traffic_pattern_features.pattern_type,
        MATCHING_LOG_FILE_SUFFIX
    );

    print_debug(&format!(
        "DEBUG [rntimatcher] going to write traffic to file: {:?}",
        matching_log_file_path
    ));
    let json_string = serde_json::to_string(traffic_collection)?;

    let mut file: File = OpenOptions::new()
        .create(true)
        .append(true)
        .open(matching_log_file_path)?;
    writeln!(file, "{}", json_string)?;

    Ok(())
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
