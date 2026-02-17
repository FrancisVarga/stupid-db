//! Individual anomaly signal scorers.
//!
//! Four detectors with weighted combination per docs/compute/algorithms/anomaly-detection.md:
//! - Signal 1: Statistical outlier (population z-score)
//! - Signal 2: DBSCAN noise ratio
//! - Signal 3: Behavioral deviation (cosine distance)
//! - Signal 4: Graph anomaly (neighbor growth, cross-community)

use stupid_core::NodeId;

use crate::algorithms::dbscan::DbscanResult;

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
pub(crate) fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
