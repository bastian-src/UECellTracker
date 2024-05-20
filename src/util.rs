#![allow(dead_code)]

use anyhow::{anyhow, Result};
use casual_logger::{Level, Log};
use lazy_static::lazy_static;
use nalgebra::{DMatrix, DVector};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::logic::rnti_matcher::TrafficCollection;

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
    Log::print_info(&format!(
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
    Log::print_info(s)
}

pub fn print_debug(s: &str) {
    if is_debug() {
        Log::print_debug(s)
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

pub fn log_rnti_matching_traffic(
    file_path: &str,
    traffic_collection: &TrafficCollection,
) -> Result<()> {
    print_debug(&format!(
        "DEBUG [rntimatcher] going to write traffic to file: {:?}",
        file_path
    ));
    let json_string = serde_json::to_string(traffic_collection)?;

    let mut file: File = OpenOptions::new()
        .create(true)
        .append(true)
        .open(file_path)?;
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

/* Feature Matching */

pub fn calculate_mean_variance(list: &[f64]) -> (f64, f64) {
    let total_packets = list.len() as f64;

    if total_packets == 0.0 {
        return (0.0, 0.0);
    }

    let mean = list.iter().sum::<f64>() / total_packets;

    let variance = list
        .iter()
        .map(|&item| {
            let diff = item - mean;
            diff * diff
        })
        .sum::<f64>()
        / total_packets;

    (mean, variance)
}

pub fn calculate_median(list: &[f64]) -> f64 {
    let mut sorted_list: Vec<f64> = list.to_vec();
    sorted_list.sort_by(|a, b| a.partial_cmp(b).unwrap()); // Handle NaN values safely
    let len = sorted_list.len();
    if len % 2 == 0 {
        // If the length is even, return the average of the two middle elements
        let mid1 = sorted_list[len / 2 - 1];
        let mid2 = sorted_list[len / 2];
        (mid1 + mid2) * 0.5
    } else {
        // If the length is odd, return the middle element
        sorted_list[len / 2]
    }
}

pub fn calculate_weighted_manhattan_distance(
    vec_a: &[f64],
    vec_b: &[f64],
    weightings: &[f64],
) -> f64 {
    assert_eq!(
        vec_a.len(),
        vec_b.len(),
        "Calcuting Euclidean distance: Vectors must have the same length"
    );
    assert_eq!(
        vec_a.len(),
        weightings.len(),
        "Calcuting Euclidean distance: Vectors and weightings must have the same length"
    );

    vec_a
        .iter()
        .zip(vec_b.iter())
        .zip(weightings.iter())
        .fold(0.0, |acc, ((&a, &b), &w)| acc + w * (a - b).abs())
}

pub fn calculate_weighted_euclidean_distance(
    vec_a: &[f64],
    vec_b: &[f64],
    weightings: &[f64],
) -> f64 {
    assert_eq!(
        vec_a.len(),
        vec_b.len(),
        "Calcuting Euclidean distance: Vectors must have the same length"
    );
    assert_eq!(
        vec_a.len(),
        weightings.len(),
        "Calcuting Euclidean distance: Vectors and weightings must have the same length"
    );

    let sum_of_squared_diff: f64 = vec_a
        .iter()
        .zip(vec_b.iter())
        .zip(weightings.iter())
        .map(|((&a, &b), &w)| w * (a - b).powi(2))
        .sum();

    sum_of_squared_diff.sqrt()
}

pub fn calculate_weighted_manhattan_distance_matrix(
    matr_a: &DMatrix<f64>,
    matr_b: &DMatrix<f64>,
    weightings: &DVector<f64>,
) -> DVector<f64> {
    assert_eq!(
        (matr_a.nrows(), matr_a.ncols()),
        (matr_b.nrows(), matr_b.ncols()),
        "Calcuting Euclidean distance: Matrices must have the same dimensions"
    );
    let diff_matrix = (matr_a - matr_b).abs();
    diff_matrix * weightings
}

pub fn calculate_weighted_euclidean_distance_matrix(
    matr_a: &DMatrix<f64>,
    matr_b: &DMatrix<f64>,
    weightings: &DVector<f64>,
) -> DVector<f64> {
    assert_eq!(
        (matr_a.nrows(), matr_a.ncols()),
        (matr_b.nrows(), matr_b.ncols()),
        "Calcuting Euclidean distance: Matrices must have the same dimensions"
    );
    let diff_matrix = matr_a - matr_b;
    let weighted_squared_diff_vector = diff_matrix.component_mul(&diff_matrix) * weightings;

    weighted_squared_diff_vector.map(|x| x.sqrt())
}

pub fn standardize_feature_vec(feature_vec: &[f64], std_vec: &[(f64, f64)]) -> Vec<f64> {
    feature_vec
        .iter()
        .zip(std_vec.iter())
        .map(|(&feature, &(mean, std_deviation))| (feature - mean) / std_deviation)
        .collect()
}
