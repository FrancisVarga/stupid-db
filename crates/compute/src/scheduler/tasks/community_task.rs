use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use stupid_graph::GraphStore;
use tracing::info;

use crate::algorithms::communities::label_propagation_default;
use crate::scheduler::state::KnowledgeState;
use crate::scheduler::task::{ComputeError, ComputeTask};
use crate::scheduler::types::{ComputeResult, Priority};

/// Wraps label propagation community detection as a schedulable compute task.
pub struct CommunityDetectionTask {
    graph: Arc<GraphStore>,
    interval: Duration,
}

impl CommunityDetectionTask {
    pub fn new(graph: Arc<GraphStore>, interval: Duration) -> Self {
        Self { graph, interval }
    }
}

impl ComputeTask for CommunityDetectionTask {
    fn name(&self) -> &str {
        "community_detection"
    }

    fn priority(&self) -> Priority {
        Priority::P2
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(3)
    }

    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
        let start = Instant::now();
        let communities = label_propagation_default(&self.graph);
        let count = communities.len();
        state.communities = communities;
        let duration = start.elapsed();

        let unique: std::collections::HashSet<u64> = state.communities.values().copied().collect();
        info!(
            "Community detection: {} nodes in {} communities ({:.1}s)",
            count,
            unique.len(),
            duration.as_secs_f64()
        );

        Ok(ComputeResult {
            task_name: self.name().to_string(),
            duration,
            items_processed: count,
            summary: Some(format!(
                "Detected {} communities across {} nodes",
                unique.len(),
                count
            )),
        })
    }

    fn should_run(&self, last_run: Option<DateTime<Utc>>, _state: &KnowledgeState) -> bool {
        match last_run {
            None => true,
            Some(last) => {
                let elapsed = Utc::now().signed_duration_since(last);
                elapsed.to_std().unwrap_or_default() >= self.interval
            }
        }
    }
}
