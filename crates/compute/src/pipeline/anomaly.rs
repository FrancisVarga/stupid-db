use stupid_core::NodeId;

use crate::algorithms::dbscan::DbscanResult;
use crate::scheduler::types::{AnomalyClassification, AnomalyResult, AnomalyScore, ClusterId};
use super::features::MemberFeatures;

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
            let std_dev = compute_std_dev(vecs, centroid, dim);
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

/// Compute per-dimension standard deviation from a set of vectors around a centroid.
fn compute_std_dev(vectors: &[Vec<f64>], centroid: &[f64], dim: usize) -> Vec<f64> {
    if vectors.len() < 2 {
        return vec![1.0; dim]; // Avoid division by zero; treat as unit std.
    }

    let n = vectors.len() as f64;
    let mut variance = vec![0.0; dim];

    for v in vectors {
        for i in 0..dim {
            let diff = v[i] - centroid[i];
            variance[i] += diff * diff;
        }
    }

    variance
        .iter()
        .map(|v| (v / n).sqrt().max(f64::EPSILON))
        .collect()
}

// ── Multi-Signal Anomaly Detection ────────────────────────────────────
//
// Four detectors with weighted combination per docs/compute/algorithms/anomaly-detection.md.

/// Default detector weights: statistical=0.2, dbscan_noise=0.3, behavioral=0.3, graph=0.2.
pub const WEIGHT_STATISTICAL: f64 = 0.2;
pub const WEIGHT_DBSCAN_NOISE: f64 = 0.3;
pub const WEIGHT_BEHAVIORAL: f64 = 0.3;
pub const WEIGHT_GRAPH: f64 = 0.2;

/// Signal 1: Population-level statistical outlier score.
///
/// For each feature dimension, compute |z| = |val - mean| / stddev.
/// Return max(|z|) / 5.0 clamped to [0, 1].
pub fn statistical_outlier_score(
    features: &[f64],
    pop_means: &[f64],
    pop_stddevs: &[f64],
) -> f64 {
    let dim = features.len().min(pop_means.len()).min(pop_stddevs.len());
    if dim == 0 {
        return 0.0;
    }

    let max_z: f64 = (0..dim)
        .filter_map(|i| {
            let std = pop_stddevs[i];
            if std <= f64::EPSILON {
                None
            } else {
                Some(((features[i] - pop_means[i]) / std).abs())
            }
        })
        .fold(0.0, f64::max);

    (max_z / 5.0).min(1.0)
}

/// Signal 2: DBSCAN noise ratio for a member.
///
/// Counts how many of the member's points were classified as noise
/// and returns noise_count / total_count. If no points found, returns 0.
pub fn dbscan_noise_score(
    member_point_ids: &[NodeId],
    dbscan_result: &DbscanResult,
) -> f64 {
    if member_point_ids.is_empty() {
        return 0.0;
    }

    let noise_count = member_point_ids
        .iter()
        .filter(|id| dbscan_result.noise.contains(id))
        .count();

    noise_count as f64 / member_point_ids.len() as f64
}

/// Signal 3: Behavioral deviation score.
///
/// Compares recent behavior to historical baseline using cosine distance.
/// Returns 1.0 - cosine_similarity(recent, baseline). High = more deviation.
pub fn behavioral_deviation_score(recent: &[f64], baseline: &[f64]) -> f64 {
    let sim = cosine_similarity(recent, baseline);
    (1.0 - sim).clamp(0.0, 1.0)
}

/// Signal 4: Graph anomaly score.
///
/// Detects unusual graph patterns:
/// - Sudden neighbor growth (>3x average)
/// - Cross-community connections (>3 communities)
pub fn graph_anomaly_score(
    neighbor_count: usize,
    avg_neighbor_count: f64,
    neighbor_community_count: usize,
) -> f64 {
    let mut score: f64 = 0.0;

    // Sudden device/connection proliferation.
    if avg_neighbor_count > 0.0 && (neighbor_count as f64) > avg_neighbor_count * 3.0 {
        score += 0.5;
    }

    // Cross-community connections.
    if neighbor_community_count > 3 {
        score += 0.3;
    }

    score.min(1.0)
}

/// Cosine similarity between two vectors. Returns 0.0 for zero-length or zero-norm vectors.
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    let dim = a.len().min(b.len());
    if dim == 0 {
        return 0.0;
    }

    let mut dot = 0.0;
    let mut norm_a = 0.0;
    let mut norm_b = 0.0;

    for i in 0..dim {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom <= f64::EPSILON {
        return 0.0;
    }

    (dot / denom).clamp(-1.0, 1.0)
}

/// Compute population-level mean and stddev per feature dimension.
///
/// Returns (means, stddevs). Stddevs are floored at EPSILON to avoid division by zero.
pub fn compute_population_stats(all_features: &[Vec<f64>]) -> (Vec<f64>, Vec<f64>) {
    if all_features.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let dim = all_features[0].len();
    let n = all_features.len() as f64;

    let mut means = vec![0.0; dim];
    for fv in all_features {
        for i in 0..dim.min(fv.len()) {
            means[i] += fv[i];
        }
    }
    for m in &mut means {
        *m /= n;
    }

    let mut variance = vec![0.0; dim];
    for fv in all_features {
        for i in 0..dim.min(fv.len()) {
            let diff = fv[i] - means[i];
            variance[i] += diff * diff;
        }
    }

    let stddevs: Vec<f64> = variance
        .iter()
        .map(|v| (v / n).sqrt().max(f64::EPSILON))
        .collect();

    (means, stddevs)
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

    // ── Multi-signal detector tests ──────────────────────────────────

    #[test]
    fn statistical_outlier_normal() {
        let features = vec![1.0, 2.0, 3.0];
        let means = vec![1.0, 2.0, 3.0];
        let stddevs = vec![1.0, 1.0, 1.0];
        let score = statistical_outlier_score(&features, &means, &stddevs);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn statistical_outlier_extreme() {
        let features = vec![11.0]; // 10 stddevs away
        let means = vec![1.0];
        let stddevs = vec![1.0];
        let score = statistical_outlier_score(&features, &means, &stddevs);
        assert_eq!(score, 1.0); // clamped: 10/5 = 2.0 -> min(2.0, 1.0)
    }

    #[test]
    fn statistical_outlier_moderate() {
        let features = vec![3.5]; // 2.5 stddevs away
        let means = vec![1.0];
        let stddevs = vec![1.0];
        let score = statistical_outlier_score(&features, &means, &stddevs);
        assert!((score - 0.5).abs() < 0.01); // 2.5/5 = 0.5
    }

    #[test]
    fn dbscan_noise_all_noise() {
        let id1 = uuid::Uuid::from_u128(1);
        let id2 = uuid::Uuid::from_u128(2);
        let result = DbscanResult {
            clusters: std::collections::HashMap::new(),
            noise: vec![id1, id2],
            num_clusters: 0,
        };
        let score = dbscan_noise_score(&[id1, id2], &result);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn dbscan_noise_none() {
        let id1 = uuid::Uuid::from_u128(1);
        let result = DbscanResult {
            clusters: [(id1, 0)].into_iter().collect(),
            noise: vec![],
            num_clusters: 1,
        };
        let score = dbscan_noise_score(&[id1], &result);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn dbscan_noise_partial() {
        let id1 = uuid::Uuid::from_u128(1);
        let id2 = uuid::Uuid::from_u128(2);
        let result = DbscanResult {
            clusters: [(id1, 0)].into_iter().collect(),
            noise: vec![id2],
            num_clusters: 1,
        };
        let score = dbscan_noise_score(&[id1, id2], &result);
        assert_eq!(score, 0.5);
    }

    #[test]
    fn behavioral_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let score = behavioral_deviation_score(&a, &a);
        assert!(score < 0.01);
    }

    #[test]
    fn behavioral_opposite() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0]; // orthogonal
        let score = behavioral_deviation_score(&a, &b);
        assert_eq!(score, 1.0);
    }

    #[test]
    fn behavioral_empty() {
        let score = behavioral_deviation_score(&[], &[]);
        assert!(score <= 1.0);
    }

    #[test]
    fn graph_anomaly_normal() {
        let score = graph_anomaly_score(5, 5.0, 1);
        assert_eq!(score, 0.0);
    }

    #[test]
    fn graph_anomaly_device_proliferation() {
        let score = graph_anomaly_score(20, 5.0, 1);
        assert_eq!(score, 0.5);
    }

    #[test]
    fn graph_anomaly_cross_community() {
        let score = graph_anomaly_score(5, 5.0, 5);
        assert_eq!(score, 0.3);
    }

    #[test]
    fn graph_anomaly_both() {
        let score = graph_anomaly_score(20, 5.0, 5);
        assert_eq!(score, 0.8);
    }

    #[test]
    fn cosine_similarity_identical() {
        let a = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &a);
        assert!((sim - 1.0).abs() < 1e-10);
    }

    #[test]
    fn cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-10);
    }

    #[test]
    fn population_stats_basic() {
        let data = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
        ];
        let (means, stddevs) = compute_population_stats(&data);
        assert_eq!(means.len(), 2);
        assert!((means[0] - 2.0).abs() < 1e-10);
        assert!((means[1] - 3.0).abs() < 1e-10);
        assert!(stddevs[0] > 0.0);
    }

    #[test]
    fn population_stats_empty() {
        let (means, stddevs) = compute_population_stats(&[]);
        assert!(means.is_empty());
        assert!(stddevs.is_empty());
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
