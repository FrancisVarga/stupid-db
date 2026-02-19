//! Template evaluator implementations.
//!
//! Each evaluator takes typed parameters plus a map of entity data,
//! returning a `Vec<RuleMatch>` of entities that triggered.
//!
//! Templates: spike, drift, absence, threshold.

use std::collections::HashMap;

use crate::schema::{
    AbsenceParams, DetectionTemplate, DriftParams, SpikeParams, ThresholdOperator, ThresholdParams,
};

use super::features::feature_index;
use super::math::{compute_mean_vector, compute_population_mean, cosine_distance, euclidean_distance};
use super::types::{ClusterStats, EntityData, RuleMatch};

// ── Dispatcher ──────────────────────────────────────────────────────

/// Dispatch evaluation to the appropriate template evaluator.
///
/// Deserializes `params` into the template-specific parameter struct and
/// delegates to the corresponding evaluator function.
pub fn evaluate_template(
    template: &DetectionTemplate,
    params: &serde_yaml::Value,
    entities: &HashMap<String, EntityData>,
    cluster_stats: &HashMap<usize, ClusterStats>,
) -> Result<Vec<RuleMatch>, String> {
    match template {
        DetectionTemplate::Spike => {
            let p: SpikeParams =
                serde_yaml::from_value(params.clone()).map_err(|e| e.to_string())?;
            Ok(evaluate_spike(&p, entities, cluster_stats))
        }
        DetectionTemplate::Drift => {
            let p: DriftParams =
                serde_yaml::from_value(params.clone()).map_err(|e| e.to_string())?;
            Ok(evaluate_drift(&p, entities))
        }
        DetectionTemplate::Absence => {
            let p: AbsenceParams =
                serde_yaml::from_value(params.clone()).map_err(|e| e.to_string())?;
            Ok(evaluate_absence(&p, entities))
        }
        DetectionTemplate::Threshold => {
            let p: ThresholdParams =
                serde_yaml::from_value(params.clone()).map_err(|e| e.to_string())?;
            Ok(evaluate_threshold(&p, entities))
        }
    }
}

// ── Spike evaluator ─────────────────────────────────────────────────

/// Detect entities whose feature value spikes above baseline x multiplier.
///
/// Baseline modes:
/// - `"cluster_centroid"` (default): compare against the entity's cluster centroid
/// - `"rolling_mean"` or `"global_mean"`: compare against the population mean
///
/// Entities with fewer than `min_samples` data points (approximated as
/// the sum of login_count + game_count) are skipped.
pub fn evaluate_spike(
    params: &SpikeParams,
    entities: &HashMap<String, EntityData>,
    cluster_stats: &HashMap<usize, ClusterStats>,
) -> Vec<RuleMatch> {
    let idx = match feature_index(&params.feature) {
        Some(i) => i,
        None => return Vec::new(),
    };

    let min_samples = params.min_samples.unwrap_or(1);
    let baseline_mode = params.baseline.as_deref().unwrap_or("cluster_centroid");

    // Precompute population mean for global/rolling baselines.
    let population_mean = compute_population_mean(entities, idx);

    let mut matches = Vec::new();

    for (entity_id, data) in entities {
        if data.features.len() <= idx {
            continue;
        }

        // Approximate sample count from login_count + game_count (indices 0 and 1).
        let sample_count = (data.features.get(0).copied().unwrap_or(0.0)
            + data.features.get(1).copied().unwrap_or(0.0)) as usize;
        if sample_count < min_samples {
            continue;
        }

        let value = data.features[idx];

        let baseline_value = match baseline_mode {
            "cluster_centroid" => {
                if let Some(cid) = data.cluster_id {
                    cluster_stats
                        .get(&cid)
                        .and_then(|cs| cs.centroid.get(idx).copied())
                        .unwrap_or(population_mean)
                } else {
                    population_mean
                }
            }
            _ => population_mean,
        };

        let threshold = baseline_value * params.multiplier;
        if value > threshold && threshold > 0.0 {
            matches.push(RuleMatch {
                entity_id: entity_id.clone(),
                entity_key: data.key.clone(),
                entity_type: data.entity_type.clone(),
                score: value,
                signals: vec![
                    (params.feature.clone(), value),
                    ("baseline".to_string(), baseline_value),
                    ("threshold".to_string(), threshold),
                ],
                matched_reason: format!(
                    "{} value {:.2} exceeds baseline {:.2} x {:.1} = {:.2}",
                    params.feature, value, baseline_value, params.multiplier, threshold,
                ),
            });
        }
    }

    matches
}

// ── Drift evaluator ─────────────────────────────────────────────────

/// Detect entities whose feature vector has drifted beyond a distance threshold.
///
/// Distance methods:
/// - `"cosine"` (default): cosine distance = 1 - cosine_similarity
/// - `"euclidean"`: Euclidean distance
///
/// The baseline is the population mean feature vector (across the requested features).
pub fn evaluate_drift(params: &DriftParams, entities: &HashMap<String, EntityData>) -> Vec<RuleMatch> {
    let indices: Vec<usize> = params
        .features
        .iter()
        .filter_map(|f| feature_index(f))
        .collect();

    if indices.is_empty() {
        return Vec::new();
    }

    let method = params.method.as_deref().unwrap_or("cosine");

    // Compute population mean vector for the selected features.
    let baseline = compute_mean_vector(entities, &indices);

    let mut matches = Vec::new();

    for (entity_id, data) in entities {
        let entity_vec: Vec<f64> = indices
            .iter()
            .map(|&i| data.features.get(i).copied().unwrap_or(0.0))
            .collect();

        let distance = match method {
            "euclidean" => euclidean_distance(&entity_vec, &baseline),
            _ => cosine_distance(&entity_vec, &baseline),
        };

        if distance > params.threshold {
            let mut signals: Vec<(String, f64)> = params
                .features
                .iter()
                .zip(entity_vec.iter())
                .map(|(name, &val)| (name.clone(), val))
                .collect();
            signals.push(("distance".to_string(), distance));

            matches.push(RuleMatch {
                entity_id: entity_id.clone(),
                entity_key: data.key.clone(),
                entity_type: data.entity_type.clone(),
                score: distance,
                signals,
                matched_reason: format!(
                    "Feature drift detected: {} distance {:.4} exceeds threshold {:.4}",
                    method, distance, params.threshold,
                ),
            });
        }
    }

    matches
}

// ── Absence evaluator ───────────────────────────────────────────────

/// Detect entities where a feature dropped to or below a threshold,
/// but only if the entity was previously active (not always zero).
///
/// "Previously active" is approximated by checking whether the entity has
/// a non-zero anomaly score or at least some login/game activity.
pub fn evaluate_absence(
    params: &AbsenceParams,
    entities: &HashMap<String, EntityData>,
) -> Vec<RuleMatch> {
    let idx = match feature_index(&params.feature) {
        Some(i) => i,
        None => return Vec::new(),
    };

    let mut matches = Vec::new();

    for (entity_id, data) in entities {
        if data.features.len() <= idx {
            continue;
        }

        let value = data.features[idx];

        // Check if entity was previously active: has non-trivial activity.
        // We use login_count + game_count + session_count as a proxy.
        let activity = data.features.get(0).copied().unwrap_or(0.0)
            + data.features.get(1).copied().unwrap_or(0.0)
            + data.features.get(6).copied().unwrap_or(0.0);

        let was_active = activity > 0.0 || data.score > 0.0;

        if was_active && value <= params.threshold {
            matches.push(RuleMatch {
                entity_id: entity_id.clone(),
                entity_key: data.key.clone(),
                entity_type: data.entity_type.clone(),
                score: value,
                signals: vec![
                    (params.feature.clone(), value),
                    ("threshold".to_string(), params.threshold),
                    ("was_active".to_string(), 1.0),
                ],
                matched_reason: format!(
                    "{} dropped to {:.2} (threshold {:.2}), entity was previously active",
                    params.feature, value, params.threshold,
                ),
            });
        }
    }

    matches
}

// ── Threshold evaluator ─────────────────────────────────────────────

/// Direct comparison of a feature value against a threshold using an operator.
pub fn evaluate_threshold(
    params: &ThresholdParams,
    entities: &HashMap<String, EntityData>,
) -> Vec<RuleMatch> {
    let idx = match feature_index(&params.feature) {
        Some(i) => i,
        None => return Vec::new(),
    };

    let mut matches = Vec::new();

    for (entity_id, data) in entities {
        if data.features.len() <= idx {
            continue;
        }

        let value = data.features[idx];
        let matched = match params.operator {
            ThresholdOperator::Gt => value > params.value,
            ThresholdOperator::Gte => value >= params.value,
            ThresholdOperator::Lt => value < params.value,
            ThresholdOperator::Lte => value <= params.value,
            ThresholdOperator::Eq => (value - params.value).abs() < f64::EPSILON,
            ThresholdOperator::Neq => (value - params.value).abs() >= f64::EPSILON,
        };

        if matched {
            matches.push(RuleMatch {
                entity_id: entity_id.clone(),
                entity_key: data.key.clone(),
                entity_type: data.entity_type.clone(),
                score: value,
                signals: vec![(params.feature.clone(), value)],
                matched_reason: format!(
                    "{} value {:.2} {:?} {:.2}",
                    params.feature, value, params.operator, params.value,
                ),
            });
        }
    }

    matches
}
