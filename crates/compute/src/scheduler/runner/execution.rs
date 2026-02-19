use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;

use chrono::Utc;
use tracing::{debug, error, info, warn};

use crate::scheduler::task::ComputeError;
use crate::scheduler::types::assess_load;

use super::Scheduler;

impl Scheduler {
    /// Execute a P0 task immediately on the current thread.
    pub fn execute_immediate(&self, task: &dyn crate::scheduler::task::ComputeTask) -> Result<(), ComputeError> {
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
}
