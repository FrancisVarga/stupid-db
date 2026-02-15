use std::sync::Arc;
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};
use stupid_graph::GraphStore;
use tokio::sync::RwLock;
use tracing::info;

use crate::algorithms::pagerank::pagerank_default;
use crate::scheduler::state::KnowledgeState;
use crate::scheduler::task::{ComputeError, ComputeTask};
use crate::scheduler::types::{ComputeResult, Priority};

/// Shared graph handle (matches server's SharedGraph type).
type SharedGraph = Arc<RwLock<GraphStore>>;

/// Wraps the PageRank algorithm as a schedulable compute task.
pub struct PageRankTask {
    graph: SharedGraph,
    interval: Duration,
}

impl PageRankTask {
    pub fn new(graph: SharedGraph, interval: Duration) -> Self {
        Self { graph, interval }
    }
}

impl ComputeTask for PageRankTask {
    fn name(&self) -> &str {
        "pagerank"
    }

    fn priority(&self) -> Priority {
        Priority::P2
    }

    fn estimated_duration(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
        let start = Instant::now();
        let graph = self.graph.blocking_read();
        let scores = pagerank_default(&graph);
        drop(graph);
        let count = scores.len();
        state.pagerank = scores;
        let duration = start.elapsed();

        info!("PageRank computed for {} nodes in {:.1}s", count, duration.as_secs_f64());

        Ok(ComputeResult {
            task_name: self.name().to_string(),
            duration,
            items_processed: count,
            summary: Some(format!("Computed PageRank for {} nodes", count)),
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
