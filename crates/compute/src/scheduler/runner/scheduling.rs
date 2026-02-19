use std::collections::HashSet;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use chrono::Utc;

use crate::scheduler::task::ComputeTask;
use crate::scheduler::types::{LoadLevel, Priority};

use super::Scheduler;

impl Scheduler {
    /// Collect tasks eligible to run given the current load level.
    pub(crate) fn collect_runnable(&self, load: LoadLevel) -> Vec<Arc<dyn ComputeTask>> {
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
