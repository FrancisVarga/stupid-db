use std::collections::HashMap;

use stupid_core::NodeId;

use crate::scheduler::types::ClusterId;

/// Streaming (online) K-means clustering.
///
/// Processes one feature vector at a time, assigning it to the nearest centroid
/// and updating that centroid incrementally. Centroids are initialized lazily
/// from the first `k` distinct points.
///
/// Designed for the hot path (Stage 2) where `< 100us` per update is required.
pub struct StreamingKMeans {
    /// Number of clusters.
    k: usize,
    /// Dimensionality of feature vectors.
    dim: usize,
    /// Current centroids. Length grows up to `k` during initialization.
    centroids: Vec<Vec<f64>>,
    /// Number of points assigned to each centroid.
    counts: Vec<usize>,
    /// Maps each member to its current cluster assignment.
    assignments: HashMap<NodeId, ClusterId>,
}

impl StreamingKMeans {
    /// Create a new streaming K-means instance.
    ///
    /// # Arguments
    /// * `k` — number of clusters (must be >= 1)
    /// * `dim` — dimensionality of feature vectors (must be >= 1)
    ///
    /// # Panics
    /// Panics if `k` or `dim` is zero.
    pub fn new(k: usize, dim: usize) -> Self {
        assert!(k >= 1, "k must be at least 1");
        assert!(dim >= 1, "dim must be at least 1");
        Self {
            k,
            dim,
            centroids: Vec::with_capacity(k),
            counts: Vec::with_capacity(k),
            assignments: HashMap::new(),
        }
    }

    /// Process a single point, assigning it to the nearest centroid and
    /// updating that centroid incrementally.
    ///
    /// During initialization (fewer than `k` centroids), each new point
    /// becomes a new centroid. After that, the nearest centroid is updated
    /// using the online mean formula: `c = c + (x - c) / n`.
    pub fn update(&mut self, member_id: NodeId, features: Vec<f64>) {
        debug_assert_eq!(
            features.len(),
            self.dim,
            "feature vector length mismatch: expected {}, got {}",
            self.dim,
            features.len()
        );

        if self.centroids.len() < self.k {
            // Initialization phase: use this point as a new centroid.
            let cluster_id = self.centroids.len() as ClusterId;
            self.centroids.push(features);
            self.counts.push(1);
            self.assignments.insert(member_id, cluster_id);
            return;
        }

        // Find the nearest centroid.
        let nearest = self.nearest_centroid(&features);

        // Increment count and update centroid with online mean.
        self.counts[nearest] += 1;
        let n = self.counts[nearest] as f64;
        let centroid = &mut self.centroids[nearest];
        for (c, x) in centroid.iter_mut().zip(features.iter()) {
            *c += (x - *c) / n;
        }

        self.assignments.insert(member_id, nearest as ClusterId);
    }

    /// Returns the cluster assignment for a member, if it has been seen.
    pub fn get_cluster(&self, member_id: &NodeId) -> Option<ClusterId> {
        self.assignments.get(member_id).copied()
    }

    /// Returns a slice of current centroids.
    pub fn centroids(&self) -> &[Vec<f64>] {
        &self.centroids
    }

    /// Returns the number of members assigned to each cluster.
    pub fn cluster_counts(&self) -> Vec<usize> {
        self.counts.clone()
    }

    /// Find the index of the nearest centroid using squared Euclidean distance.
    /// Avoids the sqrt since we only need the argmin.
    fn nearest_centroid(&self, point: &[f64]) -> usize {
        let mut best_idx = 0;
        let mut best_dist = f64::MAX;
        for (i, centroid) in self.centroids.iter().enumerate() {
            let dist = squared_euclidean(centroid, point);
            if dist < best_dist {
                best_dist = dist;
                best_idx = i;
            }
        }
        best_idx
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
    fn initialization_uses_first_k_points() {
        let mut km = StreamingKMeans::new(3, 2);

        km.update(node(1), vec![0.0, 0.0]);
        km.update(node(2), vec![10.0, 10.0]);
        km.update(node(3), vec![20.0, 20.0]);

        assert_eq!(km.centroids().len(), 3);
        assert_eq!(km.get_cluster(&node(1)), Some(0));
        assert_eq!(km.get_cluster(&node(2)), Some(1));
        assert_eq!(km.get_cluster(&node(3)), Some(2));
    }

    #[test]
    fn assigns_to_nearest_centroid() {
        let mut km = StreamingKMeans::new(2, 2);

        // Two centroids at (0,0) and (10,10).
        km.update(node(1), vec![0.0, 0.0]);
        km.update(node(2), vec![10.0, 10.0]);

        // Point near (0,0) should go to cluster 0.
        km.update(node(3), vec![1.0, 1.0]);
        assert_eq!(km.get_cluster(&node(3)), Some(0));

        // Point near (10,10) should go to cluster 1.
        km.update(node(4), vec![9.0, 9.0]);
        assert_eq!(km.get_cluster(&node(4)), Some(1));
    }

    #[test]
    fn centroid_updates_incrementally() {
        let mut km = StreamingKMeans::new(1, 2);

        km.update(node(1), vec![0.0, 0.0]);
        assert_eq!(km.centroids()[0], vec![0.0, 0.0]);

        km.update(node(2), vec![2.0, 4.0]);
        // After 2 points: centroid = (0+2)/2, (0+4)/2 = (1, 2)
        let c = &km.centroids()[0];
        assert!((c[0] - 1.0).abs() < 1e-10);
        assert!((c[1] - 2.0).abs() < 1e-10);

        km.update(node(3), vec![6.0, 6.0]);
        // After 3 points: centroid should be mean of (0,0), (2,4), (6,6) = (8/3, 10/3)
        let c = &km.centroids()[0];
        assert!((c[0] - 8.0 / 3.0).abs() < 1e-10);
        assert!((c[1] - 10.0 / 3.0).abs() < 1e-10);
    }

    #[test]
    fn cluster_counts_are_correct() {
        let mut km = StreamingKMeans::new(2, 2);

        km.update(node(1), vec![0.0, 0.0]);
        km.update(node(2), vec![10.0, 10.0]);

        // Add 3 points near cluster 0 and 1 near cluster 1.
        km.update(node(3), vec![0.5, 0.5]);
        km.update(node(4), vec![0.1, 0.1]);
        km.update(node(5), vec![9.5, 9.5]);

        let counts = km.cluster_counts();
        assert_eq!(counts[0], 3); // node(1), node(3), node(4)
        assert_eq!(counts[1], 2); // node(2), node(5)
    }

    #[test]
    fn reassignment_updates_cluster() {
        let mut km = StreamingKMeans::new(2, 1);

        km.update(node(1), vec![0.0]);
        km.update(node(2), vec![100.0]);

        // First assign node(3) near cluster 0.
        km.update(node(3), vec![1.0]);
        assert_eq!(km.get_cluster(&node(3)), Some(0));

        // Re-submit node(3) near cluster 1: assignment should change.
        km.update(node(3), vec![99.0]);
        assert_eq!(km.get_cluster(&node(3)), Some(1));
    }

    #[test]
    fn single_cluster() {
        let mut km = StreamingKMeans::new(1, 3);
        for i in 0..100 {
            km.update(node(i), vec![1.0, 2.0, 3.0]);
        }
        assert_eq!(km.centroids().len(), 1);
        assert_eq!(km.cluster_counts(), vec![100]);

        let c = &km.centroids()[0];
        assert!((c[0] - 1.0).abs() < 1e-10);
        assert!((c[1] - 2.0).abs() < 1e-10);
        assert!((c[2] - 3.0).abs() < 1e-10);
    }
}
