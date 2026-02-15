use std::collections::HashMap;

use stupid_core::NodeId;

use crate::scheduler::types::ClusterId;

/// Result of a full batch K-means run.
#[derive(Debug, Clone)]
pub struct KmeansResult {
    /// Assignment of each point to its cluster.
    pub assignments: HashMap<NodeId, ClusterId>,
    /// Final centroid vectors, indexed by cluster id.
    pub centroids: Vec<Vec<f64>>,
    /// Number of clusters.
    pub k: usize,
    /// Number of Lloyd's iterations performed.
    pub iterations: usize,
    /// Sum of squared distances from each point to its assigned centroid.
    pub inertia: f64,
}

/// Run Lloyd's K-means algorithm.
///
/// Uses K-means++ initialization for better convergence. Iterates until
/// assignments stabilize or `max_iterations` is reached.
///
/// # Arguments
/// * `points` — slice of (NodeId, feature_vector) pairs
/// * `k` — number of clusters (must be >= 1 and <= points.len())
/// * `max_iterations` — upper bound on Lloyd's iterations
///
/// # Panics
/// Panics if `k` is 0, `points` is empty, or `k > points.len()`.
pub fn kmeans(points: &[(NodeId, Vec<f64>)], k: usize, max_iterations: usize) -> KmeansResult {
    assert!(!points.is_empty(), "kmeans: points must not be empty");
    assert!(k >= 1, "kmeans: k must be at least 1");
    assert!(k <= points.len(), "kmeans: k must be <= number of points");

    let dim = points[0].1.len();
    let n = points.len();

    // K-means++ initialization.
    let mut centroids = kmeanspp_init(points, k);

    let mut assignments = vec![0usize; n];
    let mut iterations = 0;

    for _ in 0..max_iterations {
        iterations += 1;

        // Assignment step: assign each point to nearest centroid.
        let mut changed = false;
        for (i, (_id, vec)) in points.iter().enumerate() {
            let nearest = nearest_centroid(vec, &centroids);
            if assignments[i] != nearest {
                assignments[i] = nearest;
                changed = true;
            }
        }

        // If no assignments changed, we've converged.
        if !changed && iterations > 1 {
            break;
        }

        // Update step: recompute centroids as mean of assigned points.
        let mut new_centroids = vec![vec![0.0; dim]; k];
        let mut counts = vec![0usize; k];

        for (i, (_id, vec)) in points.iter().enumerate() {
            let cluster = assignments[i];
            counts[cluster] += 1;
            for (j, &val) in vec.iter().enumerate() {
                new_centroids[cluster][j] += val;
            }
        }

        for (c, centroid) in new_centroids.iter_mut().enumerate() {
            if counts[c] > 0 {
                let count = counts[c] as f64;
                for val in centroid.iter_mut() {
                    *val /= count;
                }
            } else {
                // Empty cluster: keep previous centroid.
                centroid.clone_from(&centroids[c]);
            }
        }

        centroids = new_centroids;
    }

    // Compute inertia and build assignment map.
    let mut inertia = 0.0;
    let mut assignment_map = HashMap::with_capacity(n);

    for (i, (id, vec)) in points.iter().enumerate() {
        let cluster = assignments[i];
        inertia += squared_euclidean(vec, &centroids[cluster]);
        assignment_map.insert(*id, cluster as ClusterId);
    }

    KmeansResult {
        assignments: assignment_map,
        centroids,
        k,
        iterations,
        inertia,
    }
}

/// Run K-means for each K in `k_range` and return the result with the best
/// silhouette score.
///
/// This is the recommended entry point for batch clustering when the optimal
/// K is unknown. Silhouette score ranges from -1 to 1; higher is better.
///
/// # Arguments
/// * `points` — slice of (NodeId, feature_vector) pairs (must have len >= 2)
/// * `k_range` — range of K values to try (e.g., `2..20`)
/// * `max_iterations` — max Lloyd's iterations per K
pub fn optimal_kmeans(
    points: &[(NodeId, Vec<f64>)],
    k_range: std::ops::Range<usize>,
    max_iterations: usize,
) -> KmeansResult {
    assert!(points.len() >= 2, "optimal_kmeans: need at least 2 points");

    let mut best_result: Option<KmeansResult> = None;
    let mut best_score = f64::NEG_INFINITY;

    for k in k_range {
        // Skip k values that don't make sense.
        if k < 2 || k >= points.len() {
            continue;
        }

        let result = kmeans(points, k, max_iterations);
        let score = silhouette_score(points, &result);

        if score > best_score {
            best_score = score;
            best_result = Some(result);
        }
    }

    // Fallback: if no valid K was found (e.g., range was empty or all < 2),
    // run with k=2 or k=points.len(), whichever is smaller.
    best_result.unwrap_or_else(|| {
        let k = 2.min(points.len());
        kmeans(points, k, max_iterations)
    })
}

/// Compute the mean silhouette score for a clustering result.
///
/// For each point i:
///   a(i) = average distance to other points in the same cluster
///   b(i) = minimum average distance to points in any other cluster
///   s(i) = (b(i) - a(i)) / max(a(i), b(i))
///
/// Returns the mean s(i) across all points. Range: [-1, 1].
pub fn silhouette_score(points: &[(NodeId, Vec<f64>)], result: &KmeansResult) -> f64 {
    let n = points.len();
    if n <= 1 || result.k <= 1 {
        return 0.0;
    }

    // Build cluster -> point indices map.
    let mut cluster_members: HashMap<ClusterId, Vec<usize>> = HashMap::new();
    for (i, (id, _)) in points.iter().enumerate() {
        if let Some(&cluster) = result.assignments.get(id) {
            cluster_members.entry(cluster).or_default().push(i);
        }
    }

    let mut total_silhouette = 0.0;
    let mut counted = 0;

    for (i, (id, vec_i)) in points.iter().enumerate() {
        let Some(&my_cluster) = result.assignments.get(id) else {
            continue;
        };

        let my_members = match cluster_members.get(&my_cluster) {
            Some(m) => m,
            None => continue,
        };

        // a(i): average distance to same-cluster points.
        let a = if my_members.len() <= 1 {
            0.0
        } else {
            let sum: f64 = my_members
                .iter()
                .filter(|&&j| j != i)
                .map(|&j| euclidean(vec_i, &points[j].1))
                .sum();
            sum / (my_members.len() - 1) as f64
        };

        // b(i): minimum average distance to any other cluster.
        let mut b = f64::MAX;
        for (&cid, members) in &cluster_members {
            if cid == my_cluster || members.is_empty() {
                continue;
            }
            let avg: f64 = members.iter().map(|&j| euclidean(vec_i, &points[j].1)).sum::<f64>()
                / members.len() as f64;
            if avg < b {
                b = avg;
            }
        }

        if b == f64::MAX {
            // Only one cluster with points — silhouette undefined.
            continue;
        }

        let max_ab = a.max(b);
        let s = if max_ab > 0.0 { (b - a) / max_ab } else { 0.0 };

        total_silhouette += s;
        counted += 1;
    }

    if counted == 0 {
        0.0
    } else {
        total_silhouette / counted as f64
    }
}

// ── Internal helpers ─────────────────────────────────────────

/// K-means++ initialization: pick k centroids with D²-weighted sampling.
fn kmeanspp_init(points: &[(NodeId, Vec<f64>)], k: usize) -> Vec<Vec<f64>> {
    let n = points.len();
    let mut centroids = Vec::with_capacity(k);

    // Pick first centroid: middle point (deterministic for reproducibility).
    centroids.push(points[n / 2].1.clone());

    // For remaining centroids, pick the point with max D² to existing centroids.
    // Using max-D² (greedy) instead of probabilistic sampling for determinism.
    for _ in 1..k {
        let mut best_idx = 0;
        let mut best_dist = f64::NEG_INFINITY;

        for (i, (_id, vec)) in points.iter().enumerate() {
            let min_dist = centroids
                .iter()
                .map(|c| squared_euclidean(vec, c))
                .fold(f64::MAX, f64::min);
            if min_dist > best_dist {
                best_dist = min_dist;
                best_idx = i;
            }
        }

        centroids.push(points[best_idx].1.clone());
    }

    centroids
}

/// Find the index of the nearest centroid.
fn nearest_centroid(point: &[f64], centroids: &[Vec<f64>]) -> usize {
    let mut best_idx = 0;
    let mut best_dist = f64::MAX;
    for (i, centroid) in centroids.iter().enumerate() {
        let dist = squared_euclidean(point, centroid);
        if dist < best_dist {
            best_dist = dist;
            best_idx = i;
        }
    }
    best_idx
}

/// Squared Euclidean distance.
#[inline]
fn squared_euclidean(a: &[f64], b: &[f64]) -> f64 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| {
            let d = x - y;
            d * d
        })
        .sum()
}

/// Euclidean distance (with sqrt, for silhouette).
#[inline]
fn euclidean(a: &[f64], b: &[f64]) -> f64 {
    squared_euclidean(a, b).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn node(n: u128) -> NodeId {
        Uuid::from_u128(n)
    }

    /// Helper: generate points in well-separated clusters for testing.
    fn make_clusters(centers: &[(f64, f64)], points_per_cluster: usize) -> Vec<(NodeId, Vec<f64>)> {
        let mut result = Vec::new();
        let mut id = 1u128;
        for &(cx, cy) in centers {
            for i in 0..points_per_cluster {
                let offset = (i as f64) * 0.1;
                result.push((node(id), vec![cx + offset, cy + offset]));
                id += 1;
            }
        }
        result
    }

    #[test]
    fn basic_kmeans_two_clusters() {
        let points = make_clusters(&[(0.0, 0.0), (100.0, 100.0)], 10);
        let result = kmeans(&points, 2, 100);

        assert_eq!(result.k, 2);
        assert_eq!(result.centroids.len(), 2);
        assert_eq!(result.assignments.len(), 20);

        // All points near (0,0) should be in the same cluster.
        let c0 = result.assignments[&node(1)];
        for i in 1..=10 {
            assert_eq!(result.assignments[&node(i)], c0);
        }

        // All points near (100,100) should be in a different cluster.
        let c1 = result.assignments[&node(11)];
        assert_ne!(c0, c1);
        for i in 11..=20 {
            assert_eq!(result.assignments[&node(i)], c1);
        }
    }

    #[test]
    fn kmeans_single_cluster() {
        let points = make_clusters(&[(5.0, 5.0)], 20);
        let result = kmeans(&points, 1, 100);

        assert_eq!(result.k, 1);
        assert_eq!(result.centroids.len(), 1);

        // All points should be in cluster 0.
        for (id, _) in &points {
            assert_eq!(result.assignments[id], 0);
        }
    }

    #[test]
    fn kmeans_three_clusters() {
        let points = make_clusters(&[(0.0, 0.0), (50.0, 50.0), (100.0, 100.0)], 15);
        let result = kmeans(&points, 3, 100);

        assert_eq!(result.k, 3);
        assert_eq!(result.assignments.len(), 45);

        // Verify each group has consistent assignment.
        let c0 = result.assignments[&node(1)];
        for i in 1..=15 {
            assert_eq!(result.assignments[&node(i)], c0);
        }
        let c1 = result.assignments[&node(16)];
        for i in 16..=30 {
            assert_eq!(result.assignments[&node(i)], c1);
        }
        let c2 = result.assignments[&node(31)];
        for i in 31..=45 {
            assert_eq!(result.assignments[&node(i)], c2);
        }

        // All three clusters should be distinct.
        assert_ne!(c0, c1);
        assert_ne!(c1, c2);
        assert_ne!(c0, c2);
    }

    #[test]
    fn kmeans_converges_quickly_on_separable_data() {
        let points = make_clusters(&[(0.0, 0.0), (1000.0, 1000.0)], 5);
        let result = kmeans(&points, 2, 100);

        // Well-separated data should converge in very few iterations.
        assert!(result.iterations <= 5, "iterations: {}", result.iterations);
    }

    #[test]
    fn inertia_is_non_negative() {
        let points = make_clusters(&[(0.0, 0.0), (10.0, 10.0)], 10);
        let result = kmeans(&points, 2, 100);
        assert!(result.inertia >= 0.0);
    }

    #[test]
    fn silhouette_well_separated() {
        let points = make_clusters(&[(0.0, 0.0), (100.0, 100.0)], 10);
        let result = kmeans(&points, 2, 100);
        let score = silhouette_score(&points, &result);

        // Well-separated clusters should have high silhouette score.
        assert!(score > 0.8, "silhouette score = {}", score);
    }

    #[test]
    fn silhouette_single_cluster_is_zero() {
        let points = make_clusters(&[(5.0, 5.0)], 10);
        let result = kmeans(&points, 1, 100);
        let score = silhouette_score(&points, &result);
        assert!((score - 0.0).abs() < 1e-10);
    }

    #[test]
    fn optimal_kmeans_finds_correct_k() {
        // 3 well-separated clusters — optimal_kmeans should pick k=3.
        let points = make_clusters(&[(0.0, 0.0), (100.0, 100.0), (200.0, 0.0)], 20);
        let result = optimal_kmeans(&points, 2..10, 100);

        assert_eq!(result.k, 3, "expected optimal k=3, got k={}", result.k);
    }

    #[test]
    fn optimal_kmeans_two_clusters() {
        let points = make_clusters(&[(0.0, 0.0), (100.0, 100.0)], 20);
        let result = optimal_kmeans(&points, 2..8, 100);

        assert_eq!(result.k, 2, "expected optimal k=2, got k={}", result.k);
    }

    #[test]
    fn kmeanspp_init_picks_spread_centroids() {
        let points = make_clusters(&[(0.0, 0.0), (100.0, 100.0)], 5);
        let centroids = kmeanspp_init(&points, 2);

        assert_eq!(centroids.len(), 2);

        // The two centroids should be far apart (from different groups).
        let dist = squared_euclidean(&centroids[0], &centroids[1]);
        assert!(dist > 1000.0, "centroids too close: dist²={}", dist);
    }

    #[test]
    fn kmeans_higher_dimensions() {
        // 4-dimensional data with 2 clusters.
        let mut points = Vec::new();
        for i in 0..20 {
            points.push((node(i + 1), vec![0.0, 0.0, 0.0, 0.0 + (i as f64) * 0.01]));
        }
        for i in 0..20 {
            points.push((
                node(i + 21),
                vec![100.0, 100.0, 100.0, 100.0 + (i as f64) * 0.01],
            ));
        }

        let result = kmeans(&points, 2, 100);
        assert_eq!(result.k, 2);
        assert_eq!(result.assignments.len(), 40);

        let c0 = result.assignments[&node(1)];
        let c1 = result.assignments[&node(21)];
        assert_ne!(c0, c1);
    }

    #[test]
    #[should_panic(expected = "points must not be empty")]
    fn kmeans_panics_on_empty() {
        let points: Vec<(NodeId, Vec<f64>)> = Vec::new();
        kmeans(&points, 1, 10);
    }

    #[test]
    #[should_panic(expected = "k must be at least 1")]
    fn kmeans_panics_on_zero_k() {
        let points = vec![(node(1), vec![1.0])];
        kmeans(&points, 0, 10);
    }

    #[test]
    #[should_panic(expected = "k must be <= number of points")]
    fn kmeans_panics_on_k_greater_than_n() {
        let points = vec![(node(1), vec![1.0]), (node(2), vec![2.0])];
        kmeans(&points, 3, 10);
    }
}
