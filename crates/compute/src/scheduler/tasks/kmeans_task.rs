use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use tracing::info;

use crate::algorithms::kmeans::optimal_kmeans;
use crate::scheduler::state::KnowledgeState;
use crate::scheduler::task::{ComputeError, ComputeTask};
use crate::scheduler::types::{ClusterInfo, ComputeResult, Priority};

/// Full batch K-means recompute as a schedulable P3 task.
///
/// Collects all existing cluster centroids and feature data from
/// `KnowledgeState`, runs `optimal_kmeans` with silhouette-based K
/// selection, and writes updated cluster assignments and metadata back.
///
/// This is a daily background task complementing the P0 streaming K-means.
pub struct FullKmeansTask {
    interval: Duration,
    k_range: std::ops::Range<usize>,
    max_iterations: usize,
}

impl FullKmeansTask {
    pub fn new(interval: Duration) -> Self {
        Self {
            interval,
            k_range: 2..20,
            max_iterations: 100,
        }
    }

    /// Create with custom K range and iteration limit.
    pub fn with_params(
        interval: Duration,
        k_range: std::ops::Range<usize>,
        max_iterations: usize,
    ) -> Self {
        Self {
            interval,
            k_range,
            max_iterations,
        }
    }
}

impl ComputeTask for FullKmeansTask {
    fn name(&self) -> &str {
        "full_kmeans"
    }

    fn priority(&self) -> Priority {
        Priority::P3
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(60)
    }

    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
        let start = Instant::now();

        // Collect feature vectors from cluster_info centroids.
        // Each existing cluster centroid becomes a "point" keyed by a synthetic NodeId.
        // In production, this would be fed real member feature vectors from the pipeline.
        let points: Vec<_> = state
            .cluster_info
            .iter()
            .filter(|(_, info)| !info.centroid.is_empty())
            .map(|(&_cid, info)| {
                // Use a deterministic NodeId derived from cluster id.
                let id = uuid::Uuid::from_u128(info.id as u128);
                (id, info.centroid.clone())
            })
            .collect();

        if points.len() < 2 {
            return Err(ComputeError::Skipped(format!(
                "Not enough data points for K-means ({} found, need >= 2)",
                points.len()
            )));
        }

        let k_range_end = self.k_range.end.min(points.len());
        let k_range = self.k_range.start..k_range_end;

        if k_range.is_empty() {
            return Err(ComputeError::Skipped(
                "K range is empty after clamping to data size".to_string(),
            ));
        }

        let result = optimal_kmeans(&points, k_range, self.max_iterations);

        // Write results back to KnowledgeState.
        state.clusters.clear();
        state.cluster_info.clear();

        for (id, cluster_id) in &result.assignments {
            state.clusters.insert(*id, *cluster_id);
        }

        for (cluster_idx, centroid) in result.centroids.iter().enumerate() {
            let cid = cluster_idx as u64;
            let member_count = result
                .assignments
                .values()
                .filter(|&&c| c == cid)
                .count();

            state.cluster_info.insert(
                cid,
                ClusterInfo {
                    id: cid,
                    centroid: centroid.clone(),
                    member_count,
                    label: None,
                },
            );
        }

        let duration = start.elapsed();
        let point_count = points.len();

        info!(
            "Full K-means: k={}, {} points, {} iterations, inertia={:.2} ({:.1}s)",
            result.k,
            point_count,
            result.iterations,
            result.inertia,
            duration.as_secs_f64()
        );

        Ok(ComputeResult {
            task_name: self.name().to_string(),
            duration,
            items_processed: point_count,
            summary: Some(format!(
                "K-means converged: k={}, {} points, {} iterations",
                result.k, point_count, result.iterations
            )),
        })
    }

    fn should_run(&self, last_run: Option<DateTime<Utc>>, state: &KnowledgeState) -> bool {
        // Need at least 2 cluster_info entries with centroids.
        let has_data = state
            .cluster_info
            .values()
            .filter(|info| !info.centroid.is_empty())
            .count()
            >= 2;

        if !has_data {
            return false;
        }

        match last_run {
            None => true,
            Some(last) => {
                let elapsed = Utc::now().signed_duration_since(last);
                elapsed.to_std().unwrap_or_default() >= self.interval
            }
        }
    }
}
