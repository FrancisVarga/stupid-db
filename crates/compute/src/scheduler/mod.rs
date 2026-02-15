//! Priority-based compute scheduler with backpressure and task dependencies.
//!
//! The scheduler manages execution of [`ComputeTask`] implementations across
//! a worker pool. P0 tasks run immediately, while P1-P3 tasks are checked
//! periodically based on configured intervals and current system load.
//!
//! See `docs/compute/scheduler.md` for the full design.

pub mod metrics;
pub mod runner;
pub mod state;
pub mod task;
pub mod tasks;
pub mod types;

pub use metrics::SchedulerMetrics;
pub use runner::Scheduler;
pub use state::{KnowledgeState, SharedKnowledgeState, new_shared_state};
pub use task::{ComputeError, ComputeTask};
pub use tasks::{AnomalyDetectionTask, CommunityDetectionTask, DegreeCentralityTask, FullKmeansTask, PageRankTask};
pub use types::{
    ComputeResult, LoadLevel, Priority, SchedulerConfig, assess_load,
};
