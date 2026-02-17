//! Built-in detection template evaluators.
//!
//! Templates: spike, drift, absence, threshold.
//! Each evaluator takes typed parameters plus a map of entity data,
//! returning a `Vec<RuleMatch>` of entities that triggered.

use std::collections::HashMap;

use crate::schema::{
    AbsenceParams, DetectionTemplate, DriftParams, SpikeParams, ThresholdOperator, ThresholdParams,
};

// ── Feature vector index mapping ────────────────────────────────────

/// Number of elements in the standard feature vector.
pub const FEATURE_COUNT: usize = 10;

/// Feature names in the order produced by `MemberFeatures::to_feature_vector`.
pub const FEATURE_NAMES: [&str; FEATURE_COUNT] = [
    "login_count",
    "game_count",
    "unique_games",
    "error_count",
    "popup_count",
    "platform_mobile_ratio",
    "session_count",
    "avg_session_gap_hours",
    "vip_group",
    "currency",
];

/// Map a feature name to its index in the 10-element feature vector.
///
/// Returns `None` if the name is not recognized.
/// Uses the hardcoded `FEATURE_NAMES` array; for config-driven lookup,
/// use [`feature_index_from_config`].
pub fn feature_index(name: &str) -> Option<usize> {
    FEATURE_NAMES.iter().position(|&n| n == name)
}

/// Map a feature name to its index using a compiled FeatureConfig.
///
/// Prefer this over [`feature_index`] when a loaded config is available,
/// as it reflects the actual YAML-defined feature vector.
pub fn feature_index_from_config(
    name: &str,
    config: &crate::feature_config::CompiledFeatureConfig,
) -> Option<usize> {
    config.feature_index(name)
}

/// Get feature count from a compiled FeatureConfig.
pub fn feature_count_from_config(
    config: &crate::feature_config::CompiledFeatureConfig,
) -> usize {
    config.feature_count()
}

/// Get ordered feature names from a compiled FeatureConfig.
pub fn feature_names_from_config(
    config: &crate::feature_config::CompiledFeatureConfig,
) -> &[String] {
    &config.feature_names
}

// ── Data types ──────────────────────────────────────────────────────

/// Per-entity data used as input to template evaluators.
#[derive(Debug, Clone)]
pub struct EntityData {
    /// Human-readable key (e.g., member code).
    pub key: String,
    /// Entity type label (e.g., "Member").
    pub entity_type: String,
    /// 10-element feature vector matching `FEATURE_NAMES` order.
    pub features: Vec<f64>,
    /// Anomaly score from the compute pipeline.
    pub score: f64,
    /// Cluster assignment, if available.
    pub cluster_id: Option<usize>,
}

/// Aggregate statistics for a single cluster, used as baseline in spike detection.
#[derive(Debug, Clone)]
pub struct ClusterStats {
    /// Mean feature vector (centroid) of the cluster.
    pub centroid: Vec<f64>,
    /// Number of members in the cluster.
    pub member_count: usize,
}

/// A single detection match produced by a template evaluator.
#[derive(Debug, Clone)]
pub struct RuleMatch {
    /// Node ID as string.
    pub entity_id: String,
    /// Human-readable key (e.g., member code).
    pub entity_key: String,
    /// Entity type label.
    pub entity_type: String,
    /// Detection score (typically the feature value or distance).
    pub score: f64,
    /// Signals that contributed to the match: (name, value) pairs.
    pub signals: Vec<(String, f64)>,
    /// Human-readable explanation of why this entity matched.
    pub matched_reason: String,
}

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

// ── Helper functions ────────────────────────────────────────────────

/// Compute the population mean for a single feature across all entities.
fn compute_population_mean(entities: &HashMap<String, EntityData>, feature_idx: usize) -> f64 {
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
fn compute_mean_vector(entities: &HashMap<String, EntityData>, indices: &[usize]) -> Vec<f64> {
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
fn cosine_distance(a: &[f64], b: &[f64]) -> f64 {
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
fn euclidean_distance(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(&ai, &bi)| (ai - bi).powi(2))
        .sum::<f64>()
        .sqrt()
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{DetectionTemplate, ThresholdOperator};

    /// Build a test entity with the given feature values.
    fn make_entity(key: &str, features: Vec<f64>, cluster_id: Option<usize>) -> EntityData {
        EntityData {
            key: key.to_string(),
            entity_type: "Member".to_string(),
            features,
            score: 0.5,
            cluster_id,
        }
    }

    /// Default 10-element feature vector with all zeros.
    fn zero_features() -> Vec<f64> {
        vec![0.0; FEATURE_COUNT]
    }

    #[test]
    fn feature_index_known_names() {
        assert_eq!(feature_index("login_count"), Some(0));
        assert_eq!(feature_index("game_count"), Some(1));
        assert_eq!(feature_index("unique_games"), Some(2));
        assert_eq!(feature_index("error_count"), Some(3));
        assert_eq!(feature_index("popup_count"), Some(4));
        assert_eq!(feature_index("platform_mobile_ratio"), Some(5));
        assert_eq!(feature_index("session_count"), Some(6));
        assert_eq!(feature_index("avg_session_gap_hours"), Some(7));
        assert_eq!(feature_index("vip_group"), Some(8));
        assert_eq!(feature_index("currency"), Some(9));
        assert_eq!(feature_index("nonexistent"), None);
    }

    #[test]
    fn spike_detection_with_known_data() {
        let mut entities = HashMap::new();

        // Normal entity: login_count=10, game_count=5
        let mut normal = zero_features();
        normal[0] = 10.0; // login_count
        normal[1] = 5.0; // game_count
        entities.insert("e1".to_string(), make_entity("M001", normal, Some(0)));

        // Spike entity: login_count=100, game_count=5
        let mut spike = zero_features();
        spike[0] = 100.0;
        spike[1] = 5.0;
        entities.insert("e2".to_string(), make_entity("M002", spike, Some(0)));

        let mut cluster_stats = HashMap::new();
        let mut centroid = zero_features();
        centroid[0] = 10.0; // cluster mean login_count = 10
        cluster_stats.insert(
            0,
            ClusterStats {
                centroid,
                member_count: 2,
            },
        );

        let params = SpikeParams {
            feature: "login_count".to_string(),
            multiplier: 3.0,
            baseline: Some("cluster_centroid".to_string()),
            min_samples: Some(5),
        };

        let results = evaluate_spike(&params, &entities, &cluster_stats);

        // Only entity e2 should match: 100 > 10 * 3 = 30
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M002");
        assert_eq!(results[0].score, 100.0);
        assert!(results[0].matched_reason.contains("login_count"));
    }

    #[test]
    fn spike_skips_low_sample_entities() {
        let mut entities = HashMap::new();

        // Entity with login_count=0, game_count=2 => samples = 2
        let mut feat = zero_features();
        feat[0] = 0.0;
        feat[1] = 2.0;
        feat[3] = 50.0; // error_count is high
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let params = SpikeParams {
            feature: "error_count".to_string(),
            multiplier: 2.0,
            baseline: Some("global_mean".to_string()),
            min_samples: Some(5),
        };

        let results = evaluate_spike(&params, &entities, &HashMap::new());
        assert_eq!(results.len(), 0, "Entity with 2 samples should be skipped");
    }

    #[test]
    fn threshold_all_six_operators() {
        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 10.0; // login_count = 10
        feat[1] = 5.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let cases = vec![
            (ThresholdOperator::Gt, 9.0, true),
            (ThresholdOperator::Gt, 10.0, false),
            (ThresholdOperator::Gte, 10.0, true),
            (ThresholdOperator::Gte, 11.0, false),
            (ThresholdOperator::Lt, 11.0, true),
            (ThresholdOperator::Lt, 10.0, false),
            (ThresholdOperator::Lte, 10.0, true),
            (ThresholdOperator::Lte, 9.0, false),
            (ThresholdOperator::Eq, 10.0, true),
            (ThresholdOperator::Eq, 10.1, false),
            (ThresholdOperator::Neq, 10.1, true),
            (ThresholdOperator::Neq, 10.0, false),
        ];

        for (op, value, expected) in cases {
            let params = ThresholdParams {
                feature: "login_count".to_string(),
                operator: op.clone(),
                value,
            };
            let results = evaluate_threshold(&params, &entities);
            assert_eq!(
                !results.is_empty(),
                expected,
                "login_count=10 {:?} {}: expected match={}",
                op,
                value,
                expected
            );
        }
    }

    #[test]
    fn drift_cosine_distance() {
        let mut entities = HashMap::new();

        // Entity close to baseline
        let mut close = zero_features();
        close[0] = 10.0;
        close[1] = 10.0;
        entities.insert("e1".to_string(), make_entity("M001", close, None));

        // Entity far from baseline (orthogonal-ish)
        let mut far = zero_features();
        far[0] = 0.0;
        far[1] = 100.0;
        entities.insert("e2".to_string(), make_entity("M002", far, None));

        let params = DriftParams {
            features: vec!["login_count".to_string(), "game_count".to_string()],
            method: Some("cosine".to_string()),
            threshold: 0.05, // very tight threshold
            window: None,
            baseline_window: None,
        };

        let results = evaluate_drift(&params, &entities);

        // The mean is (5, 55). e1=(10,10) vs mean=(5,55) has large cosine distance.
        // e2=(0,100) vs mean=(5,55) has smaller cosine distance.
        // At least one should exceed 0.05.
        assert!(
            !results.is_empty(),
            "At least one entity should show cosine drift"
        );
    }

    #[test]
    fn drift_euclidean_distance() {
        let mut entities = HashMap::new();

        let mut feat = zero_features();
        feat[0] = 100.0;
        feat[1] = 0.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let mut feat2 = zero_features();
        feat2[0] = 0.0;
        feat2[1] = 0.0;
        entities.insert("e2".to_string(), make_entity("M002", feat2, None));

        let params = DriftParams {
            features: vec!["login_count".to_string()],
            method: Some("euclidean".to_string()),
            threshold: 40.0,
            window: None,
            baseline_window: None,
        };

        let results = evaluate_drift(&params, &entities);
        // Mean login_count = 50. e1 is at 100 (dist=50 > 40), e2 is at 0 (dist=50 > 40).
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn absence_previously_active_entity() {
        let mut entities = HashMap::new();

        // Previously active entity whose error_count dropped to 0
        let mut feat = zero_features();
        feat[0] = 20.0; // login_count (shows activity)
        feat[1] = 10.0; // game_count
        feat[3] = 0.0; // error_count dropped to 0
        feat[6] = 5.0; // session_count
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        // Entity that was never active (all zeros, score 0)
        let inactive = EntityData {
            key: "M002".to_string(),
            entity_type: "Member".to_string(),
            features: zero_features(),
            score: 0.0,
            cluster_id: None,
        };
        // Ensure login + game + session = 0, score = 0
        entities.insert("e2".to_string(), inactive);

        let params = AbsenceParams {
            feature: "error_count".to_string(),
            threshold: 1.0,
            lookback_days: 7,
            compare_to: None,
        };

        let results = evaluate_absence(&params, &entities);

        // Only e1 should match (was active, error_count=0 <= 1.0)
        // e2 should NOT match (was never active)
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
        assert!(results[0].matched_reason.contains("previously active"));
    }

    #[test]
    fn absence_does_not_match_always_zero() {
        let mut entities = HashMap::new();

        let data = EntityData {
            key: "M_INACTIVE".to_string(),
            entity_type: "Member".to_string(),
            features: zero_features(),
            score: 0.0,
            cluster_id: None,
        };
        entities.insert("e1".to_string(), data);

        let params = AbsenceParams {
            feature: "login_count".to_string(),
            threshold: 1.0,
            lookback_days: 7,
            compare_to: None,
        };

        let results = evaluate_absence(&params, &entities);
        assert!(
            results.is_empty(),
            "Entity with no prior activity should not trigger absence"
        );
    }

    #[test]
    fn evaluate_template_dispatcher() {
        let mut entities = HashMap::new();
        let mut feat = zero_features();
        feat[0] = 50.0;
        feat[1] = 10.0;
        entities.insert("e1".to_string(), make_entity("M001", feat, None));

        let params_yaml = serde_yaml::to_value(&ThresholdParams {
            feature: "login_count".to_string(),
            operator: ThresholdOperator::Gte,
            value: 25.0,
        })
        .unwrap();

        let results = evaluate_template(
            &DetectionTemplate::Threshold,
            &params_yaml,
            &entities,
            &HashMap::new(),
        )
        .unwrap();

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entity_key, "M001");
    }

    #[test]
    fn evaluate_template_bad_params() {
        let entities = HashMap::new();
        let bad_yaml = serde_yaml::Value::String("not a struct".to_string());

        let result = evaluate_template(
            &DetectionTemplate::Spike,
            &bad_yaml,
            &entities,
            &HashMap::new(),
        );

        assert!(result.is_err(), "Bad params should return Err");
    }

    #[test]
    fn unknown_feature_returns_empty() {
        let mut entities = HashMap::new();
        entities.insert(
            "e1".to_string(),
            make_entity("M001", zero_features(), None),
        );

        let params = ThresholdParams {
            feature: "nonexistent_feature".to_string(),
            operator: ThresholdOperator::Gt,
            value: 0.0,
        };

        let results = evaluate_threshold(&params, &entities);
        assert!(results.is_empty());
    }

    #[test]
    fn cosine_distance_identical_vectors() {
        let a = vec![1.0, 2.0, 3.0];
        let d = cosine_distance(&a, &a);
        assert!(d.abs() < 1e-10, "Distance to self should be ~0");
    }

    #[test]
    fn cosine_distance_orthogonal_vectors() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = cosine_distance(&a, &b);
        assert!((d - 1.0).abs() < 1e-10, "Orthogonal vectors should have distance ~1.0");
    }
}
