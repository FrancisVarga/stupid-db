use std::time::Duration;

use chrono::{DateTime, Utc};

use super::state::KnowledgeState;
use super::types::{ComputeResult, Priority};

/// Error type for compute task execution.
#[derive(Debug, thiserror::Error)]
pub enum ComputeError {
    #[error("Task failed: {0}")]
    Failed(String),
    #[error("Task skipped: {0}")]
    Skipped(String),
    #[error("Lock poisoned: {0}")]
    LockPoisoned(String),
}

/// A unit of compute work that the scheduler can execute.
///
/// Implementations wrap specific algorithms (PageRank, DBSCAN, etc.)
/// and write results into [`KnowledgeState`].
pub trait ComputeTask: Send + Sync {
    /// Human-readable name for logging and metrics.
    fn name(&self) -> &str;

    /// Execution priority level.
    fn priority(&self) -> Priority;

    /// Estimated execution duration (used by scheduler for planning).
    fn estimated_duration(&self) -> Duration;

    /// Execute the compute task, writing results into state.
    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError>;

    /// Whether this task should run now, given when it last ran and current state.
    fn should_run(&self, last_run: Option<DateTime<Utc>>, state: &KnowledgeState) -> bool;
}
