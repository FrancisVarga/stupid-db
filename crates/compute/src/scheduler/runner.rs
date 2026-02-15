use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use chrono::{DateTime, Utc};
use tracing::{debug, error, info, warn};

use super::metrics::SchedulerMetrics;
use super::state::SharedKnowledgeState;
use super::task::{ComputeError, ComputeTask};
use super::types::{LoadLevel, Priority, SchedulerConfig, TaskDependency, assess_load};

/// The compute scheduler. Manages a pool of workers and executes
/// [`ComputeTask`] implementations based on priority and backpressure.
pub struct Scheduler {
    config: SchedulerConfig,
    /// Registered periodic tasks (P1-P3).
    registered_tasks: Vec<Arc<dyn ComputeTask>>,
    /// Task dependency edges.
    dependencies: Vec<TaskDependency>,
    /// Shared knowledge state written to by tasks.
    state: SharedKnowledgeState,
    /// Scheduler metrics.
    metrics: Arc<RwLock<SchedulerMetrics>>,
    /// Last run time per task name.
    last_run: Arc<RwLock<HashMap<String, DateTime<Utc>>>>,
    /// External ingest queue depth signal.
    ingest_queue_depth: Arc<AtomicUsize>,
    /// Shutdown signal.
    shutdown: Arc<AtomicBool>,
    /// Active worker count (for utilization tracking).
    active_workers: Arc<AtomicUsize>,
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
        self.ingest_queue_depth.store(depth, Ordering::Relaxed);
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
        self.shutdown.store(true, Ordering::Relaxed);
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

    /// Execute a P0 task immediately on the current thread.
    pub fn execute_immediate(&self, task: &dyn ComputeTask) -> Result<(), ComputeError> {
        debug!("Executing P0 task: {}", task.name());
        let mut state = self.state.write().map_err(|e| {
            ComputeError::LockPoisoned(format!("KnowledgeState write lock: {}", e))
        })?;

        let result = task.execute(&mut state)?;

        // Record metrics
        if let Ok(mut metrics) = self.metrics.write() {
            metrics.record_execution(task.name(), result.duration);
        }

        if let Ok(mut last_run) = self.last_run.write() {
            last_run.insert(task.name().to_string(), Utc::now());
        }

        Ok(())
    }

    /// Run the main scheduling loop. Blocks until shutdown is signaled.
    ///
    /// Uses a thread pool via `rayon` for parallel task execution.
    pub fn run(&self) {
        let num_workers = self.config.resolved_worker_threads();
        info!(
            "Scheduler starting with {} workers, {} registered tasks",
            num_workers,
            self.registered_tasks.len()
        );

        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_workers)
            .build()
            .expect("Failed to build rayon thread pool");

        while !self.shutdown.load(Ordering::Relaxed) {
            let queue_depth = self.ingest_queue_depth.load(Ordering::Relaxed);
            let load = assess_load(queue_depth, &self.config);

            // Update metrics
            if let Ok(mut m) = self.metrics.write() {
                m.current_load_level = load;
                m.ingest_queue_depth = queue_depth;
                let active = self.active_workers.load(Ordering::Relaxed);
                m.worker_utilization = active as f64 / num_workers as f64;
            }

            // Collect tasks that should run this tick
            let runnable = self.collect_runnable(load);

            // Execute runnable tasks on the thread pool
            for task in runnable {
                let state = Arc::clone(&self.state);
                let metrics = Arc::clone(&self.metrics);
                let last_run = Arc::clone(&self.last_run);
                let active_workers = Arc::clone(&self.active_workers);

                pool.spawn(move || {
                    active_workers.fetch_add(1, Ordering::Relaxed);

                    let result = {
                        let mut state_guard = match state.write() {
                            Ok(g) => g,
                            Err(e) => {
                                error!("Failed to acquire state lock for {}: {}", task.name(), e);
                                active_workers.fetch_sub(1, Ordering::Relaxed);
                                return;
                            }
                        };
                        task.execute(&mut state_guard)
                    };

                    match result {
                        Ok(r) => {
                            debug!("Task {} completed in {:?}", task.name(), r.duration);
                            if let Ok(mut m) = metrics.write() {
                                m.record_execution(task.name(), r.duration);
                            }
                            if let Ok(mut lr) = last_run.write() {
                                lr.insert(task.name().to_string(), Utc::now());
                            }
                        }
                        Err(e) => {
                            warn!("Task {} failed: {}", task.name(), e);
                        }
                    }

                    active_workers.fetch_sub(1, Ordering::Relaxed);
                });
            }

            std::thread::sleep(Duration::from_millis(100));
        }

        info!("Scheduler stopped");
    }

    /// Collect tasks eligible to run given the current load level.
    fn collect_runnable(&self, load: LoadLevel) -> Vec<Arc<dyn ComputeTask>> {
        let last_run = self.last_run.read().unwrap();
        let state = self.state.read().unwrap();
        let active = self.active_workers.load(Ordering::Relaxed);
        let num_workers = self.config.resolved_worker_threads();
        let available = num_workers.saturating_sub(active);

        // Build set of completed tasks (those that have a last_run entry)
        let completed_tasks: HashSet<&str> = last_run.keys().map(|s| s.as_str()).collect();

        let mut runnable = Vec::new();

        for task in &self.registered_tasks {
            let priority = task.priority();

            // Backpressure filtering
            match load {
                LoadLevel::Critical if priority == Priority::P2 || priority == Priority::P3 => {
                    continue;
                }
                LoadLevel::Elevated if priority == Priority::P3 => continue,
                LoadLevel::Elevated if priority == Priority::P2 => {
                    // P2 at half frequency: check if double interval has elapsed
                    let interval = self.config.interval_for(priority);
                    let double_interval = interval * 2;
                    let last = last_run.get(task.name()).copied();
                    if let Some(last) = last {
                        let elapsed = Utc::now().signed_duration_since(last);
                        if elapsed.to_std().unwrap_or_default() < double_interval {
                            continue;
                        }
                    }
                }
                _ => {}
            }

            // Worker availability check per docs:
            // P2 needs > 2 available, P3 needs > 4 available
            match priority {
                Priority::P2 if available <= 2 => continue,
                Priority::P3 if available <= 4 => continue,
                _ => {}
            }

            // Dependency check
            if !self.dependencies_met(task.name(), &completed_tasks) {
                continue;
            }

            // should_run check
            let last = last_run.get(task.name()).copied();
            if task.should_run(last, &state) {
                runnable.push(Arc::clone(task));
            }
        }

        runnable
    }

    /// Check whether all dependencies for a task have been satisfied.
    fn dependencies_met(&self, task_name: &str, completed: &HashSet<&str>) -> bool {
        for dep in &self.dependencies {
            if dep.to == task_name && !completed.contains(dep.from.as_str()) {
                return false;
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scheduler::state::{KnowledgeState, new_shared_state};
    use crate::scheduler::task::ComputeTask;
    use crate::scheduler::types::ComputeResult;
    use std::sync::atomic::AtomicUsize;

    /// Mock compute task for testing.
    struct MockTask {
        name: String,
        priority: Priority,
        execute_count: Arc<AtomicUsize>,
        always_run: bool,
    }

    impl MockTask {
        fn new(name: &str, priority: Priority) -> Self {
            Self {
                name: name.to_string(),
                priority,
                execute_count: Arc::new(AtomicUsize::new(0)),
                always_run: true,
            }
        }

        fn with_controlled_run(name: &str, priority: Priority) -> Self {
            Self {
                name: name.to_string(),
                priority,
                execute_count: Arc::new(AtomicUsize::new(0)),
                always_run: false,
            }
        }

        fn execution_count(&self) -> usize {
            self.execute_count.load(Ordering::Relaxed)
        }
    }

    impl ComputeTask for MockTask {
        fn name(&self) -> &str { &self.name }
        fn priority(&self) -> Priority { self.priority }
        fn estimated_duration(&self) -> Duration { Duration::from_millis(10) }

        fn execute(&self, _state: &mut KnowledgeState) -> Result<ComputeResult, crate::scheduler::task::ComputeError> {
            self.execute_count.fetch_add(1, Ordering::Relaxed);
            Ok(ComputeResult {
                task_name: self.name.clone(),
                duration: Duration::from_millis(1),
                items_processed: 1,
                summary: None,
            })
        }

        fn should_run(&self, _last_run: Option<DateTime<Utc>>, _state: &KnowledgeState) -> bool {
            self.always_run
        }
    }

    #[test]
    fn scheduler_creation() {
        let state = new_shared_state();
        let scheduler = Scheduler::new(SchedulerConfig::default(), state);
        let metrics = scheduler.metrics();
        assert_eq!(metrics.current_load_level, LoadLevel::Normal);
        assert_eq!(metrics.ingest_queue_depth, 0);
    }

    #[test]
    fn register_task() {
        let state = new_shared_state();
        let mut scheduler = Scheduler::new(SchedulerConfig::default(), state);
        let task = Arc::new(MockTask::new("test", Priority::P1));
        scheduler.register_task(task);
        assert_eq!(scheduler.registered_tasks.len(), 1);
    }

    #[test]
    fn execute_immediate_p0() {
        let state = new_shared_state();
        let scheduler = Scheduler::new(SchedulerConfig::default(), state);
        let task = MockTask::new("p0_task", Priority::P0);

        scheduler.execute_immediate(&task).unwrap();

        assert_eq!(task.execution_count(), 1);
        let metrics = scheduler.metrics();
        assert_eq!(metrics.tasks_executed["p0_task"], 1);
    }

    #[test]
    fn backpressure_critical_blocks_p2_p3() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 10; // enough workers
        let mut scheduler = Scheduler::new(config, state);

        let p1 = Arc::new(MockTask::new("p1_task", Priority::P1));
        let p2 = Arc::new(MockTask::new("p2_task", Priority::P2));
        let p3 = Arc::new(MockTask::new("p3_task", Priority::P3));

        scheduler.register_task(p1);
        scheduler.register_task(p2);
        scheduler.register_task(p3);

        let runnable = scheduler.collect_runnable(LoadLevel::Critical);
        let names: Vec<&str> = runnable.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"p1_task"), "P1 should run under Critical");
        assert!(!names.contains(&"p2_task"), "P2 should be blocked under Critical");
        assert!(!names.contains(&"p3_task"), "P3 should be blocked under Critical");
    }

    #[test]
    fn backpressure_elevated_blocks_p3() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 10;
        let mut scheduler = Scheduler::new(config, state);

        let p1 = Arc::new(MockTask::new("p1_task", Priority::P1));
        let p3 = Arc::new(MockTask::new("p3_task", Priority::P3));

        scheduler.register_task(p1);
        scheduler.register_task(p3);

        let runnable = scheduler.collect_runnable(LoadLevel::Elevated);
        let names: Vec<&str> = runnable.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"p1_task"), "P1 should run under Elevated");
        assert!(!names.contains(&"p3_task"), "P3 should be blocked under Elevated");
    }

    #[test]
    fn dependency_enforcement() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 10;
        let mut scheduler = Scheduler::new(config, state);

        let entity = Arc::new(MockTask::new("entity_extraction", Priority::P1));
        let pagerank = Arc::new(MockTask::new("pagerank", Priority::P2));

        scheduler.register_task(entity.clone());
        scheduler.register_task(pagerank);
        scheduler.add_dependency("entity_extraction", "pagerank");

        // Before entity_extraction runs, pagerank should be blocked
        let runnable = scheduler.collect_runnable(LoadLevel::Normal);
        let names: Vec<&str> = runnable.iter().map(|t| t.name()).collect();

        assert!(names.contains(&"entity_extraction"));
        assert!(!names.contains(&"pagerank"), "pagerank should be blocked by dependency");

        // Simulate entity_extraction having run
        scheduler.last_run.write().unwrap().insert(
            "entity_extraction".to_string(),
            Utc::now(),
        );

        let runnable = scheduler.collect_runnable(LoadLevel::Normal);
        let names: Vec<&str> = runnable.iter().map(|t| t.name()).collect();
        assert!(names.contains(&"pagerank"), "pagerank should run after dependency met");
    }

    #[test]
    fn should_run_gate() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 10;
        let mut scheduler = Scheduler::new(config, state);

        let task = Arc::new(MockTask::with_controlled_run("gated", Priority::P1));
        scheduler.register_task(task);

        let runnable = scheduler.collect_runnable(LoadLevel::Normal);
        assert!(runnable.is_empty(), "task with should_run=false shouldn't be collected");
    }

    #[test]
    fn shutdown_and_run_lifecycle() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 2;
        let mut scheduler = Scheduler::new(config, state);

        let task = Arc::new(MockTask::new("lifecycle_task", Priority::P1));
        scheduler.register_task(task.clone());

        // Immediately signal shutdown before running
        scheduler.shutdown();

        // run() should return immediately since shutdown is set
        scheduler.run();

        // Task may or may not have run (depends on timing),
        // but the scheduler should have exited cleanly
    }

    #[test]
    fn ingest_queue_depth_signal() {
        let state = new_shared_state();
        let scheduler = Scheduler::new(SchedulerConfig::default(), state);

        scheduler.set_ingest_queue_depth(5000);
        assert_eq!(
            scheduler.ingest_queue_depth.load(Ordering::Relaxed),
            5000
        );
    }

    #[test]
    fn worker_availability_gates_p2_p3() {
        let state = new_shared_state();
        let mut config = SchedulerConfig::default();
        config.worker_threads = 3; // Only 3 workers, need >2 for P2
        let mut scheduler = Scheduler::new(config, state);

        let p2 = Arc::new(MockTask::new("p2_task", Priority::P2));
        scheduler.register_task(p2);

        // With 0 active workers, available = 3 which is > 2
        let runnable = scheduler.collect_runnable(LoadLevel::Normal);
        assert_eq!(runnable.len(), 1, "P2 should run with 3 available workers");

        // Simulate 1 active worker: available = 2, need > 2
        scheduler.active_workers.store(1, Ordering::Relaxed);
        let runnable = scheduler.collect_runnable(LoadLevel::Normal);
        assert!(runnable.is_empty(), "P2 shouldn't run with only 2 available workers");
    }
}
