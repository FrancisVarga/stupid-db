use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use stupid_graph::GraphStore;
use tracing::info;

use crate::algorithms::degree::degree_centrality;
use crate::scheduler::state::KnowledgeState;
use crate::scheduler::task::{ComputeError, ComputeTask};
use crate::scheduler::types::{ComputeResult, Priority};

/// Wraps degree centrality computation as a schedulable compute task.
pub struct DegreeCentralityTask {
    graph: Arc<GraphStore>,
    interval: Duration,
}

impl DegreeCentralityTask {
    pub fn new(graph: Arc<GraphStore>, interval: Duration) -> Self {
        Self { graph, interval }
    }
}

impl ComputeTask for DegreeCentralityTask {
    fn name(&self) -> &str {
        "degree_centrality"
    }

    fn priority(&self) -> Priority {
        Priority::P2
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(1)
    }

    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
        let start = Instant::now();
        let degrees = degree_centrality(&self.graph);
        let count = degrees.len();
        state.degrees = degrees;
        let duration = start.elapsed();

        info!("Degree centrality computed for {} nodes in {:.1}s", count, duration.as_secs_f64());

        Ok(ComputeResult {
            task_name: self.name().to_string(),
            duration,
            items_processed: count,
            summary: Some(format!("Computed degree centrality for {} nodes", count)),
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
