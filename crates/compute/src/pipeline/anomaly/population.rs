//! Population-level statistics for anomaly detection.
//!
//! Provides mean/variance/stddev computation across feature vectors,
//! used both for population-level outlier scoring and per-cluster
//! standard deviation estimation.

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

/// Compute per-dimension standard deviation from a set of vectors around a centroid.
pub(crate) fn compute_std_dev(vectors: &[Vec<f64>], centroid: &[f64], dim: usize) -> Vec<f64> {
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

#[cfg(test)]
mod tests {
    use super::*;

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
    fn std_dev_single_vector() {
        let vectors = vec![vec![1.0, 2.0]];
        let centroid = vec![1.0, 2.0];
        let result = compute_std_dev(&vectors, &centroid, 2);
        // With < 2 vectors, returns unit std.
        assert_eq!(result, vec![1.0, 1.0]);
    }

    #[test]
    fn std_dev_multiple_vectors() {
        let vectors = vec![
            vec![1.0, 2.0],
            vec![3.0, 4.0],
        ];
        let centroid = vec![2.0, 3.0];
        let result = compute_std_dev(&vectors, &centroid, 2);
        // Each dim: sqrt(((1-2)^2 + (3-2)^2) / 2) = sqrt(1) = 1.0
        assert!((result[0] - 1.0).abs() < 1e-10);
        assert!((result[1] - 1.0).abs() < 1e-10);
    }
}
