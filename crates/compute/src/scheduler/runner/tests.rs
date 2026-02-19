#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    use chrono::{DateTime, Utc};

    use crate::scheduler::runner::Scheduler;
    use crate::scheduler::state::{KnowledgeState, new_shared_state};
    use crate::scheduler::task::{ComputeError, ComputeTask};
    use crate::scheduler::types::{ComputeResult, LoadLevel, Priority, SchedulerConfig};

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

        fn execute(&self, _state: &mut KnowledgeState) -> Result<ComputeResult, ComputeError> {
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
