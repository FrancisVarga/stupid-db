use std::collections::{HashMap, VecDeque};

use stupid_core::NodeId;

use crate::scheduler::types::ClusterId;

/// Result of DBSCAN clustering.
#[derive(Debug, Clone)]
pub struct DbscanResult {
    /// Maps each clustered point to its cluster ID.
    pub clusters: HashMap<NodeId, ClusterId>,
    /// Points classified as noise (not belonging to any cluster).
    pub noise: Vec<NodeId>,
    /// Total number of clusters found.
    pub num_clusters: usize,
}

/// Run DBSCAN density-based clustering on a set of points with dense feature vectors.
///
/// # Arguments
/// * `points` — slice of `(NodeId, feature_vector)` pairs
/// * `eps` — neighborhood radius (Euclidean distance)
/// * `min_pts` — minimum number of neighbors (including the point itself) to form a core point
///
/// # Returns
/// A [`DbscanResult`] containing cluster assignments and noise points.
pub fn dbscan(points: &[(NodeId, Vec<f64>)], eps: f64, min_pts: usize) -> DbscanResult {
    let n = points.len();
    if n == 0 {
        return DbscanResult {
            clusters: HashMap::new(),
            noise: Vec::new(),
            num_clusters: 0,
        };
    }

    let eps_sq = eps * eps;

    // Pre-compute pairwise neighbor lists to avoid redundant distance calculations.
    let neighbors: Vec<Vec<usize>> = (0..n)
        .map(|i| {
            (0..n)
                .filter(|&j| squared_euclidean(&points[i].1, &points[j].1) <= eps_sq)
                .collect()
        })
        .collect();

    let mut labels: Vec<Option<ClusterId>> = vec![None; n];
    let mut visited = vec![false; n];
    let mut current_cluster: ClusterId = 0;

    for i in 0..n {
        if visited[i] {
            continue;
        }
        visited[i] = true;

        if neighbors[i].len() < min_pts {
            // Not a core point; tentatively noise (may be claimed by a cluster later).
            continue;
        }

        // Start a new cluster from this core point.
        labels[i] = Some(current_cluster);

        let mut queue: VecDeque<usize> = neighbors[i]
            .iter()
            .copied()
            .filter(|&j| j != i)
            .collect();

        while let Some(j) = queue.pop_front() {
            if labels[j].is_none() {
                labels[j] = Some(current_cluster);
            }

            if visited[j] {
                continue;
            }
            visited[j] = true;

            if neighbors[j].len() >= min_pts {
                // j is also a core point: expand the cluster with its neighbors.
                for &nb in &neighbors[j] {
                    if labels[nb].is_none() {
                        queue.push_back(nb);
                    }
                }
            }
        }

        current_cluster += 1;
    }

    let mut clusters = HashMap::new();
    let mut noise = Vec::new();
    for (idx, label) in labels.iter().enumerate() {
        match label {
            Some(c) => {
                clusters.insert(points[idx].0, *c);
            }
            None => {
                noise.push(points[idx].0);
            }
        }
    }

    DbscanResult {
        clusters,
        noise,
        num_clusters: current_cluster as usize,
    }
}

/// Squared Euclidean distance between two vectors.
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

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn node(n: u128) -> NodeId {
        Uuid::from_u128(n)
    }

    #[test]
    fn empty_input() {
        let result = dbscan(&[], 1.0, 2);
        assert_eq!(result.num_clusters, 0);
        assert!(result.clusters.is_empty());
        assert!(result.noise.is_empty());
    }

    #[test]
    fn single_point_is_noise() {
        let points = vec![(node(1), vec![0.0, 0.0])];
        let result = dbscan(&points, 1.0, 2);
        assert_eq!(result.num_clusters, 0);
        assert!(result.clusters.is_empty());
        assert_eq!(result.noise.len(), 1);
    }

    #[test]
    fn two_clusters_well_separated() {
        // Cluster A around (0,0), Cluster B around (100,100).
        let points = vec![
            (node(1), vec![0.0, 0.0]),
            (node(2), vec![1.0, 0.0]),
            (node(3), vec![0.0, 1.0]),
            (node(4), vec![100.0, 100.0]),
            (node(5), vec![101.0, 100.0]),
            (node(6), vec![100.0, 101.0]),
        ];

        let result = dbscan(&points, 2.0, 2);

        assert_eq!(result.num_clusters, 2);
        assert!(result.noise.is_empty());

        // All points in cluster A should share the same cluster ID.
        let c_a = result.clusters[&node(1)];
        assert_eq!(result.clusters[&node(2)], c_a);
        assert_eq!(result.clusters[&node(3)], c_a);

        // All points in cluster B should share a different cluster ID.
        let c_b = result.clusters[&node(4)];
        assert_eq!(result.clusters[&node(5)], c_b);
        assert_eq!(result.clusters[&node(6)], c_b);

        assert_ne!(c_a, c_b);
    }

    #[test]
    fn noise_points_detected() {
        // Two tight clusters + one outlier.
        let points = vec![
            (node(1), vec![0.0, 0.0]),
            (node(2), vec![0.5, 0.0]),
            (node(3), vec![0.0, 0.5]),
            (node(10), vec![50.0, 50.0]), // outlier
            (node(4), vec![10.0, 10.0]),
            (node(5), vec![10.5, 10.0]),
            (node(6), vec![10.0, 10.5]),
        ];

        let result = dbscan(&points, 1.0, 2);

        assert_eq!(result.num_clusters, 2);
        assert_eq!(result.noise.len(), 1);
        assert_eq!(result.noise[0], node(10));
    }

    #[test]
    fn min_pts_one_means_all_clustered() {
        // With min_pts=1 and large eps, every point is a core point.
        let points = vec![
            (node(1), vec![0.0]),
            (node(2), vec![100.0]),
            (node(3), vec![200.0]),
        ];

        let result = dbscan(&points, 1000.0, 1);

        assert!(result.noise.is_empty());
        // All should be in a single cluster since eps covers all pairwise distances.
        assert_eq!(result.num_clusters, 1);
    }

    #[test]
    fn chain_connectivity() {
        // Points in a chain: each is within eps of its neighbor,
        // but endpoints are far from each other. DBSCAN should
        // connect them into one cluster via density reachability.
        let points: Vec<(NodeId, Vec<f64>)> = (0..10)
            .map(|i| (node(i as u128), vec![i as f64 * 1.0]))
            .collect();

        let result = dbscan(&points, 1.5, 2);

        assert_eq!(result.num_clusters, 1);
        assert!(result.noise.is_empty());
    }

    #[test]
    fn high_min_pts_makes_everything_noise() {
        let points = vec![
            (node(1), vec![0.0, 0.0]),
            (node(2), vec![1.0, 0.0]),
            (node(3), vec![0.0, 1.0]),
        ];

        // min_pts=10: no point has 10 neighbors within eps=1.0.
        let result = dbscan(&points, 1.0, 10);

        assert_eq!(result.num_clusters, 0);
        assert_eq!(result.noise.len(), 3);
    }

    #[test]
    fn border_point_assigned_to_cluster() {
        // Core points at (0,0) and (1,0); border point at (2,0)
        // is within eps of (1,0) but doesn't have enough neighbors to be core.
        let points = vec![
            (node(1), vec![0.0]),
            (node(2), vec![0.5]),
            (node(3), vec![1.0]),
            (node(4), vec![2.0]), // border: within eps=1.5 of node(3) but only 2 neighbors
        ];

        let result = dbscan(&points, 1.2, 2);

        // node(4) should be assigned to the cluster, not be noise.
        assert!(
            result.clusters.contains_key(&node(4)),
            "border point should be assigned to a cluster"
        );
    }
}
