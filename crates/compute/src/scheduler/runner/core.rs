use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize};
use std::sync::{Arc, RwLock};

use chrono::{DateTime, Utc};
use tracing::info;

use crate::scheduler::metrics::SchedulerMetrics;
use crate::scheduler::state::SharedKnowledgeState;
use crate::scheduler::task::ComputeTask;
use crate::scheduler::types::{SchedulerConfig, TaskDependency};

/// The compute scheduler. Manages a pool of workers and executes
/// [`ComputeTask`] implementations based on priority and backpressure.
pub struct Scheduler {
    pub(super) config: SchedulerConfig,
    /// Registered periodic tasks (P1-P3).
    pub(super) registered_tasks: Vec<Arc<dyn ComputeTask>>,
    /// Task dependency edges.
    pub(super) dependencies: Vec<TaskDependency>,
    /// Shared knowledge state written to by tasks.
    pub(super) state: SharedKnowledgeState,
    /// Scheduler metrics.
    pub(super) metrics: Arc<RwLock<SchedulerMetrics>>,
    /// Last run time per task name.
    pub(super) last_run: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// External ingest queue depth signal.
    pub(super) ingest_queue_depth: Arc<AtomicUsize>,
    /// Shutdown signal.
    pub(super) shutdown: Arc<AtomicBool>,
    /// Active worker count (for utilization tracking).
    pub(super) active_workers: Arc<AtomicUsize>,
}

impl Scheduler {
    /// Create a new scheduler with the given config and shared state.
    pub fn new(config: SchedulerConfig, state: SharedKnowledgeState) -> Self {
        Self {
            config,
            registered_tasks: Vec::new(),
            dependencies: Vec::new(),
            state,
            metrics: Arc::new(RwLock::new(SchedulerMetrics::default())),
            last_run: Arc::new(RwLock::new(HashMap::new())),
            ingest_queue_depth: Arc::new(AtomicUsize::new(0)),
            shutdown: Arc::new(AtomicBool::new(false)),
            active_workers: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Register a periodic task (P1-P3) with the scheduler.
    pub fn register_task(&mut self, task: Arc<dyn ComputeTask>) {
        info!("Registered task: {} (priority: {:?})", task.name(), task.priority());
        self.registered_tasks.push(task);
    }

    /// Add a task dependency: `from` must complete before `to` can run.
    pub fn add_dependency(&mut self, from: &str, to: &str) {
        self.dependencies.push(TaskDependency {
            from: from.to_string(),
            to: to.to_string(),
        });
    }

    /// Update the ingest queue depth signal (called by the ingest pipeline).
    pub fn set_ingest_queue_depth(&self, depth: usize) {
        self.ingest_queue_depth.store(depth, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get a snapshot of the current scheduler metrics.
    pub fn metrics(&self) -> SchedulerMetrics {
        self.metrics.read().unwrap().clone()
    }

    /// Get a handle to the shared knowledge state.
    pub fn knowledge_state(&self) -> SharedKnowledgeState {
        Arc::clone(&self.state)
    }

    /// Signal the scheduler to stop.
    pub fn shutdown(&self) {
        info!("Scheduler shutdown requested");
        self.shutdown.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get an Arc to the shutdown flag (for external shutdown signaling).
    pub fn shutdown_signal(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.shutdown)
    }

    /// Get an Arc to the metrics (for external reads without cloning).
    pub fn metrics_handle(&self) -> Arc<RwLock<SchedulerMetrics>> {
        Arc::clone(&self.metrics)
    }

    /// Get a reference to the registered tasks.
    pub fn registered_tasks(&self) -> &[Arc<dyn ComputeTask>] {
        &self.registered_tasks
    }
}
