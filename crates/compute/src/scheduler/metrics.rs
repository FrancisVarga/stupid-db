use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::types::{LoadLevel, Priority};

/// Scheduler operational metrics exposed to the dashboard.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulerMetrics {
    /// Total tasks executed by name.
    pub tasks_executed: HashMap<String, u64>,
    /// Number of tasks pending per priority level.
    pub tasks_pending: HashMap<Priority, usize>,
    /// Worker utilization ratio (0.0 - 1.0).
    pub worker_utilization: f64,
    /// Average task duration by task name.
    pub avg_task_duration: HashMap<String, Duration>,
    /// Last execution time by task name.
    pub last_run: HashMap<String, DateTime<Utc>>,
    /// Current system load level.
    pub current_load_level: LoadLevel,
    /// Current ingest queue depth.
    pub ingest_queue_depth: usize,
}

impl Default for SchedulerMetrics {
    fn default() -> Self {
        Self {
            tasks_executed: HashMap::new(),
            tasks_pending: HashMap::new(),
            worker_utilization: 0.0,
            avg_task_duration: HashMap::new(),
            last_run: HashMap::new(),
            current_load_level: LoadLevel::Normal,
            ingest_queue_depth: 0,
        }
    }
}

impl SchedulerMetrics {
    /// Record a task execution.
    pub fn record_execution(&mut self, task_name: &str, duration: Duration) {
        *self.tasks_executed.entry(task_name.to_string()).or_default() += 1;
        self.last_run
            .insert(task_name.to_string(), Utc::now());

        // Update rolling average duration
        let count = self.tasks_executed[task_name];
        let prev_avg = self
            .avg_task_duration
            .get(task_name)
            .copied()
            .unwrap_or_default();

        // Incremental mean: new_avg = prev_avg + (duration - prev_avg) / count
        let new_avg = if count == 1 {
            duration
        } else {
            let prev_nanos = prev_avg.as_nanos() as f64;
            let cur_nanos = duration.as_nanos() as f64;
            let avg_nanos = prev_nanos + (cur_nanos - prev_nanos) / count as f64;
            Duration::from_nanos(avg_nanos as u64)
        };

        self.avg_task_duration
            .insert(task_name.to_string(), new_avg);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_single_execution() {
        let mut m = SchedulerMetrics::default();
        m.record_execution("test_task", Duration::from_millis(100));

        assert_eq!(m.tasks_executed["test_task"], 1);
        assert!(m.last_run.contains_key("test_task"));
        assert_eq!(m.avg_task_duration["test_task"], Duration::from_millis(100));
    }

    #[test]
    fn record_multiple_executions_averages() {
        let mut m = SchedulerMetrics::default();
        m.record_execution("task", Duration::from_millis(100));
        m.record_execution("task", Duration::from_millis(200));

        assert_eq!(m.tasks_executed["task"], 2);
        // Average of 100ms and 200ms = 150ms
        let avg = m.avg_task_duration["task"].as_millis();
        assert!((140..=160).contains(&avg), "expected ~150ms, got {}ms", avg);
    }

    #[test]
    fn default_metrics() {
        let m = SchedulerMetrics::default();
        assert_eq!(m.current_load_level, LoadLevel::Normal);
        assert_eq!(m.ingest_queue_depth, 0);
        assert_eq!(m.worker_utilization, 0.0);
        assert!(m.tasks_executed.is_empty());
    }
}
