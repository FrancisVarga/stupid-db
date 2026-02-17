//! Anomaly detection pipeline — orchestration and scoring.
//!
//! Combines cluster-based z-score anomaly detection with a multi-signal
//! approach (statistical, DBSCAN noise, behavioral, graph) to produce
//! composite anomaly results.
//!
//! Sub-modules:
//! - [`signals`] — individual signal scorer functions
//! - [`population`] — population-level statistics (mean, variance, std-dev)

pub mod population;
pub mod signals;

use stupid_core::NodeId;

use crate::algorithms::dbscan::DbscanResult;
use crate::scheduler::types::{AnomalyClassification, AnomalyResult, AnomalyScore, ClusterId};
use super::features::MemberFeatures;

// ── Re-exports ────────────────────────────────────────────────────────
// Preserve existing `pipeline::anomaly::*` import paths.
pub use signals::{
    behavioral_deviation_score, dbscan_noise_score, graph_anomaly_score,
    statistical_outlier_score,
};
pub use population::compute_population_stats;

/// Default threshold above which a member is considered anomalous.
const DEFAULT_ANOMALY_THRESHOLD: f64 = 2.0;

/// Trait abstracting cluster assignment and centroid access.
///
/// Implemented by `StreamingKMeans` and test doubles. Decouples anomaly
/// scoring from the concrete clustering algorithm.
pub trait ClusterProvider {
    /// Return the cluster a member is assigned to, if any.
    fn get_cluster(&self, member_id: &NodeId) -> Option<ClusterId>;

    /// Return the current cluster centroids.
    fn centroids(&self) -> &[Vec<f64>];
}

// Implement the trait for the concrete StreamingKMeans type.
impl ClusterProvider for crate::algorithms::streaming_kmeans::StreamingKMeans {
    fn get_cluster(&self, member_id: &NodeId) -> Option<ClusterId> {
        self.get_cluster(member_id)
    }

    fn centroids(&self) -> &[Vec<f64>] {
        self.centroids()
    }
}

/// Default detector weights: statistical=0.2, dbscan_noise=0.3, behavioral=0.3, graph=0.2.
pub const WEIGHT_STATISTICAL: f64 = 0.2;
pub const WEIGHT_DBSCAN_NOISE: f64 = 0.3;
pub const WEIGHT_BEHAVIORAL: f64 = 0.3;
pub const WEIGHT_GRAPH: f64 = 0.2;

/// Compute an anomaly score for a single member based on z-score deviation
/// from its cluster centroid.
///
/// Score = mean of per-dimension |feature_i - centroid_i| / std_i.
/// Dimensions where std_i is zero are skipped (no variance = no anomaly signal).
pub fn compute_anomaly_score(
    member_features: &[f64],
    centroid: &[f64],
    cluster_std: &[f64],
) -> AnomalyScore {
    compute_anomaly_score_with_threshold(
        member_features,
        centroid,
        cluster_std,
        DEFAULT_ANOMALY_THRESHOLD,
    )
}

/// Compute anomaly score with a configurable threshold.
pub fn compute_anomaly_score_with_threshold(
    member_features: &[f64],
    centroid: &[f64],
    cluster_std: &[f64],
    threshold: f64,
) -> AnomalyScore {
    let dim = member_features
        .len()
        .min(centroid.len())
        .min(cluster_std.len());

    if dim == 0 {
        return AnomalyScore {
            score: 0.0,
            is_anomalous: false,
        };
    }

    let mut z_sum = 0.0;
    let mut valid_dims = 0usize;

    for i in 0..dim {
        let std_i = cluster_std[i];
        if std_i <= f64::EPSILON {
            continue;
        }
        z_sum += (member_features[i] - centroid[i]).abs() / std_i;
        valid_dims += 1;
    }

    let score = if valid_dims > 0 {
        z_sum / valid_dims as f64
    } else {
        0.0
    };

    AnomalyScore {
        score,
        is_anomalous: score > threshold,
    }
}

/// Score all tracked members against their assigned cluster centroids.
///
/// For each member with a cluster assignment, computes the z-score anomaly.
/// Cluster standard deviations are estimated from all members in that cluster.
pub fn score_all_members<C: ClusterProvider>(
    features: &MemberFeatures,
    kmeans: &C,
) -> Vec<(NodeId, AnomalyScore)> {
    let centroids = kmeans.centroids();
    if centroids.is_empty() {
        return Vec::new();
    }

    let dim = centroids[0].len();

    // First pass: collect all feature vectors grouped by cluster to compute std.
    let mut cluster_vectors: std::collections::HashMap<ClusterId, Vec<Vec<f64>>> =
        std::collections::HashMap::new();

    let members: Vec<NodeId> = features.members().copied().collect();

    for member_id in &members {
        if let (Some(cluster_id), Some(fv)) =
            (kmeans.get_cluster(member_id), features.to_feature_vector(member_id))
        {
            cluster_vectors
                .entry(cluster_id)
                .or_default()
                .push(fv);
        }
    }

    // Compute per-cluster standard deviations.
    let cluster_stds: std::collections::HashMap<ClusterId, Vec<f64>> = cluster_vectors
        .iter()
        .map(|(&cid, vecs)| {
            let centroid = &centroids[cid as usize];
            let std_dev = population::compute_std_dev(vecs, centroid, dim);
            (cid, std_dev)
        })
        .collect();

    // Second pass: score each member.
    let mut results = Vec::with_capacity(members.len());
    for member_id in &members {
        if let (Some(cluster_id), Some(fv)) =
            (kmeans.get_cluster(member_id), features.to_feature_vector(member_id))
        {
            let centroid = &centroids[cluster_id as usize];
            let std_dev = match cluster_stds.get(&cluster_id) {
                Some(s) => s,
                None => continue,
            };
            let score = compute_anomaly_score(&fv, centroid, std_dev);
            results.push((*member_id, score));
        }
    }

    results
}

/// Combine four detector signals into a weighted anomaly result.
pub fn multi_signal_score(
    statistical: f64,
    dbscan_noise: f64,
    behavioral: f64,
    graph: f64,
) -> AnomalyResult {
    let score = statistical * WEIGHT_STATISTICAL
        + dbscan_noise * WEIGHT_DBSCAN_NOISE
        + behavioral * WEIGHT_BEHAVIORAL
        + graph * WEIGHT_GRAPH;

    let score = score.clamp(0.0, 1.0);
    let classification = AnomalyClassification::from_score(score);

    AnomalyResult {
        score,
        classification,
        signals: vec![
            ("statistical".to_string(), statistical),
            ("dbscan_noise".to_string(), dbscan_noise),
            ("behavioral".to_string(), behavioral),
            ("graph".to_string(), graph),
        ],
    }
}

/// Combine four detector signals using weights from a ScoringConfig.
pub fn multi_signal_score_with_config(
    statistical: f64,
    dbscan_noise: f64,
    behavioral: f64,
    graph: f64,
    config: &stupid_rules::scoring_config::CompiledScoringConfig,
) -> AnomalyResult {
    let w = &config.multi_signal_weights;
    let score = statistical * w.statistical
        + dbscan_noise * w.dbscan_noise
        + behavioral * w.behavioral
        + graph * w.graph;

    let score = score.clamp(0.0, 1.0);
    let classification = classify_score_with_config(score, &config.classification_thresholds);

    AnomalyResult {
        score,
        classification,
        signals: vec![
            ("statistical".to_string(), statistical),
            ("dbscan_noise".to_string(), dbscan_noise),
            ("behavioral".to_string(), behavioral),
            ("graph".to_string(), graph),
        ],
    }
}

/// Classify an anomaly score using config-driven thresholds.
pub fn classify_score_with_config(
    score: f64,
    thresholds: &stupid_rules::scoring_config::ClassificationThresholds,
) -> AnomalyClassification {
    if score >= thresholds.highly_anomalous {
        AnomalyClassification::HighlyAnomalous
    } else if score >= thresholds.anomalous {
        AnomalyClassification::Anomalous
    } else if score >= thresholds.mild {
        AnomalyClassification::Mild
    } else {
        AnomalyClassification::Normal
    }
}

/// Score all members using the multi-signal approach.
///
/// This is the main entry point for the anomaly detection pipeline.
/// It runs all available detectors and combines the results.
///
/// `dbscan_result` and `community_map` are optional — if None, those signals score 0.
pub fn multi_signal_score_all<C: ClusterProvider>(
    features: &MemberFeatures,
    kmeans: &C,
    dbscan_result: Option<&DbscanResult>,
    community_map: Option<&std::collections::HashMap<NodeId, u64>>,
    avg_neighbor_count: f64,
) -> Vec<(NodeId, AnomalyResult)> {
    // Collect all feature vectors for population stats.
    let members: Vec<NodeId> = features.members().copied().collect();
    let all_fvs: Vec<Vec<f64>> = members
        .iter()
        .filter_map(|id| features.to_feature_vector(id))
        .collect();

    if all_fvs.is_empty() {
        return Vec::new();
    }

    let (pop_means, pop_stddevs) = compute_population_stats(&all_fvs);

    let mut results = Vec::with_capacity(members.len());

    for member_id in &members {
        let fv = match features.to_feature_vector(member_id) {
            Some(v) => v,
            None => continue,
        };

        // Signal 1: Statistical outlier.
        let s1 = statistical_outlier_score(&fv, &pop_means, &pop_stddevs);

        // Signal 2: DBSCAN noise ratio.
        let s2 = match dbscan_result {
            Some(db) => {
                // Single member = single point in feature space.
                if db.noise.contains(member_id) {
                    1.0
                } else {
                    0.0
                }
            }
            None => 0.0,
        };

        // Signal 3: Behavioral deviation.
        // Without temporal windowing, use cluster centroid as baseline.
        let s3 = if let Some(cluster_id) = kmeans.get_cluster(member_id) {
            let centroids = kmeans.centroids();
            if (cluster_id as usize) < centroids.len() {
                behavioral_deviation_score(&fv, &centroids[cluster_id as usize])
            } else {
                0.0
            }
        } else {
            0.0
        };

        // Signal 4: Graph anomaly.
        let s4 = if let Some(_cm) = community_map {
            // Without graph store access, we can't query actual neighbors.
            // Graph anomaly scoring requires neighbor data passed in separately.
            // For now, score 0 — the scheduler task will provide real data.
            graph_anomaly_score(0, avg_neighbor_count, 0)
        } else {
            0.0
        };

        results.push((*member_id, multi_signal_score(s1, s2, s3, s4)));
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normal_point_not_anomalous() {
        let features = vec![1.0, 2.0, 3.0];
        let centroid = vec![1.0, 2.0, 3.0];
        let std_dev = vec![1.0, 1.0, 1.0];

        let score = compute_anomaly_score(&features, &centroid, &std_dev);
        assert_eq!(score.score, 0.0);
        assert!(!score.is_anomalous);
    }

    #[test]
    fn outlier_is_anomalous() {
        let features = vec![10.0, 20.0, 30.0];
        let centroid = vec![1.0, 2.0, 3.0];
        let std_dev = vec![1.0, 1.0, 1.0];

        let score = compute_anomaly_score(&features, &centroid, &std_dev);
        assert!(score.score > 2.0);
        assert!(score.is_anomalous);
    }

    #[test]
    fn zero_std_dims_are_skipped() {
        let features = vec![5.0, 2.0];
        let centroid = vec![1.0, 2.0];
        let std_dev = vec![0.0, 1.0]; // first dim has no variance

        let score = compute_anomaly_score(&features, &centroid, &std_dev);
        // Only second dim contributes: |2-2|/1 = 0
        assert_eq!(score.score, 0.0);
    }

    #[test]
    fn empty_features() {
        let score = compute_anomaly_score(&[], &[], &[]);
        assert_eq!(score.score, 0.0);
        assert!(!score.is_anomalous);
    }

    #[test]
    fn multi_signal_normal() {
        let result = multi_signal_score(0.0, 0.0, 0.0, 0.0);
        assert_eq!(result.score, 0.0);
        assert_eq!(result.classification, AnomalyClassification::Normal);
        assert_eq!(result.signals.len(), 4);
    }

    #[test]
    fn multi_signal_highly_anomalous() {
        let result = multi_signal_score(1.0, 1.0, 1.0, 1.0);
        assert_eq!(result.score, 1.0);
        assert_eq!(result.classification, AnomalyClassification::HighlyAnomalous);
    }

    #[test]
    fn multi_signal_mixed() {
        // 0.2*0.5 + 0.3*0.8 + 0.3*0.0 + 0.2*0.0 = 0.1 + 0.24 = 0.34
        let result = multi_signal_score(0.5, 0.8, 0.0, 0.0);
        assert!((result.score - 0.34).abs() < 1e-10);
        assert_eq!(result.classification, AnomalyClassification::Mild);
    }

    #[test]
    fn classification_thresholds() {
        assert_eq!(AnomalyClassification::from_score(0.0), AnomalyClassification::Normal);
        assert_eq!(AnomalyClassification::from_score(0.29), AnomalyClassification::Normal);
        assert_eq!(AnomalyClassification::from_score(0.3), AnomalyClassification::Mild);
        assert_eq!(AnomalyClassification::from_score(0.49), AnomalyClassification::Mild);
        assert_eq!(AnomalyClassification::from_score(0.5), AnomalyClassification::Anomalous);
        assert_eq!(AnomalyClassification::from_score(0.69), AnomalyClassification::Anomalous);
        assert_eq!(AnomalyClassification::from_score(0.7), AnomalyClassification::HighlyAnomalous);
        assert_eq!(AnomalyClassification::from_score(1.0), AnomalyClassification::HighlyAnomalous);
    }
}
