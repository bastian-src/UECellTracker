use anyhow::{anyhow, Result};
use nalgebra::{DMatrix, DVector};

/* Feature Matching */

pub fn calculate_mean_variance(list: &[f64]) -> Result<(f64, f64)> {
    let total_packets = list.len() as f64;

    if total_packets <= 0.0 {
        return Err(anyhow!("Cannot determine mean/variance of 0 length array"));
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

    Ok((mean, variance))
}

pub fn calculate_median(list: &[f64]) -> Result<f64> {
    let len: usize = list.len();
    if len == 0 {
        return Err(anyhow!("Cannot determine median of 0 length array"));
    }
    let mut sorted_list: Vec<f64> = list.to_vec();
    sorted_list.sort_by(|a, b| a.partial_cmp(b).unwrap()); // Handle NaN values safely
    if len % 2 == 0 {
        // If the length is even, return the average of the two middle elements
        let mid1 = sorted_list[len / 2 - 1];
        let mid2 = sorted_list[len / 2];
        Ok((mid1 + mid2) * 0.5)
    } else {
        // If the length is odd, return the middle element
        Ok(sorted_list[len / 2])
    }
}

#[allow(dead_code)]
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

#[allow(dead_code)]
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
