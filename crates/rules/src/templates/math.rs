//! Mathematical helper functions for template evaluators.
//!
//! Population statistics and distance metrics used by spike and drift detection.

use std::collections::HashMap;

use super::types::EntityData;

/// Compute the population mean for a single feature across all entities.
pub(super) fn compute_population_mean(
    entities: &HashMap<String, EntityData>,
    feature_idx: usize,
) -> f64 {
    let mut sum = 0.0;
    let mut count = 0usize;
    for data in entities.values() {
        if let Some(&v) = data.features.get(feature_idx) {
            sum += v;
            count += 1;
        }
    }
    if count > 0 {
        sum / count as f64
    } else {
        0.0
    }
}

/// Compute the mean feature vector across all entities for the given feature indices.
pub(super) fn compute_mean_vector(
    entities: &HashMap<String, EntityData>,
    indices: &[usize],
) -> Vec<f64> {
    let n = entities.len() as f64;
    if n == 0.0 {
        return vec![0.0; indices.len()];
    }

    let mut sums = vec![0.0; indices.len()];
    for data in entities.values() {
        for (j, &idx) in indices.iter().enumerate() {
            sums[j] += data.features.get(idx).copied().unwrap_or(0.0);
        }
    }

    sums.iter().map(|s| s / n).collect()
}

/// Cosine distance: 1.0 - cosine_similarity.
///
/// Returns 1.0 if either vector has zero magnitude.
pub(super) fn cosine_distance(a: &[f64], b: &[f64]) -> f64 {
    let mut dot = 0.0;
    let mut mag_a = 0.0;
    let mut mag_b = 0.0;

    for (&ai, &bi) in a.iter().zip(b.iter()) {
        dot += ai * bi;
        mag_a += ai * ai;
        mag_b += bi * bi;
    }

    let denom = mag_a.sqrt() * mag_b.sqrt();
    if denom < f64::EPSILON {
        return 1.0;
    }

    1.0 - (dot / denom)
}

/// Euclidean distance between two vectors.
pub(super) fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| (ai - bi).powi(2))
        .sum::<f64>()
        .sqrt()
}
