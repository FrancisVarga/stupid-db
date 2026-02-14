use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use stupid_core::NodeId;

/// Task execution priority. Lower numeric value = higher priority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub enum Priority {
    /// Realtime — runs synchronously with ingest hot path.
    P0 = 0,
    /// Near-realtime — every few minutes on recent batches.
    P1 = 1,
    /// Periodic — hourly on broader data windows.
    P2 = 2,
    /// Background — daily, expensive, tolerates delay.
    P3 = 3,
}

/// System load level, determined by ingest queue depth.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LoadLevel {
    /// All priorities active.
    Normal,
    /// P3 paused, P2 at half frequency.
    Elevated,
    /// P2+P3 paused, only P0+P1.
    Critical,
}

/// Assess current load level from ingest queue depth.
pub fn assess_load(ingest_queue_depth: usize, config: &SchedulerConfig) -> LoadLevel {
    if ingest_queue_depth > config.critical_threshold {
        LoadLevel::Critical
    } else if ingest_queue_depth > config.backpressure_threshold {
        LoadLevel::Elevated
    } else {
        LoadLevel::Normal
    }
}

/// Result of executing a compute task.
#[derive(Debug, Clone, Serialize)]
pub struct ComputeResult {
    /// Name of the task that produced this result.
    pub task_name: String,
    /// How long the task took.
    pub duration: Duration,
    /// Number of items processed (nodes, edges, documents, etc.).
    pub items_processed: usize,
    /// Optional human-readable summary.
    pub summary: Option<String>,
}

/// Scheduler configuration, typically parsed from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    /// Number of worker threads. 0 = num_cpus.
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
    /// P1 task interval in seconds.
    #[serde(default = "default_p1_interval")]
    pub p1_interval_seconds: u64,
    /// P2 task interval in seconds.
    #[serde(default = "default_p2_interval")]
    pub p2_interval_seconds: u64,
    /// P3 task interval in seconds.
    #[serde(default = "default_p3_interval")]
    pub p3_interval_seconds: u64,
    /// Queue depth triggering elevated backpressure.
    #[serde(default = "default_backpressure")]
    pub backpressure_threshold: usize,
    /// Queue depth triggering critical backpressure.
    #[serde(default = "default_critical")]
    pub critical_threshold: usize,
}

fn default_worker_threads() -> usize { 0 }
fn default_p1_interval() -> u64 { 300 }
fn default_p2_interval() -> u64 { 3600 }
fn default_p3_interval() -> u64 { 86400 }
fn default_backpressure() -> usize { 1000 }
fn default_critical() -> usize { 10000 }

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            worker_threads: default_worker_threads(),
            p1_interval_seconds: default_p1_interval(),
            p2_interval_seconds: default_p2_interval(),
            p3_interval_seconds: default_p3_interval(),
            backpressure_threshold: default_backpressure(),
            critical_threshold: default_critical(),
        }
    }
}

impl SchedulerConfig {
    /// Resolve worker thread count (0 means use available parallelism).
    pub fn resolved_worker_threads(&self) -> usize {
        if self.worker_threads == 0 {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        } else {
            self.worker_threads
        }
    }

    /// Get the configured interval for a given priority level.
    pub fn interval_for(&self, priority: Priority) -> Duration {
        match priority {
            Priority::P0 => Duration::ZERO,
            Priority::P1 => Duration::from_secs(self.p1_interval_seconds),
            Priority::P2 => Duration::from_secs(self.p2_interval_seconds),
            Priority::P3 => Duration::from_secs(self.p3_interval_seconds),
        }
    }
}

// ── Placeholder domain types ─────────────────────────────────
// These will be fleshed out as algorithms are implemented.

/// Unique identifier for a cluster.
pub type ClusterId = u64;

/// Unique identifier for a community.
pub type CommunityId = u64;

/// Metadata about a cluster.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterInfo {
    pub id: ClusterId,
    pub centroid: Vec<f64>,
    pub member_count: usize,
    pub label: Option<String>,
}

/// Anomaly score for a node.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AnomalyScore {
    pub score: f64,
    pub is_anomalous: bool,
}

/// A temporal pattern detected across events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemporalPattern {
    pub id: String,
    pub description: String,
    pub confidence: f64,
    pub occurrences: usize,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
}

/// A detected trend in a metric.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trend {
    pub metric_name: String,
    pub direction: TrendDirection,
    pub magnitude: f64,
    pub baseline: f64,
    pub current: f64,
}

/// Direction of a trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    Rising,
    Falling,
    Stable,
}

/// A proactive insight generated by the system.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Insight {
    pub id: String,
    pub title: String,
    pub description: String,
    pub severity: InsightSeverity,
    pub created_at: DateTime<Utc>,
    pub related_nodes: Vec<NodeId>,
}

/// Severity of an insight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsightSeverity {
    Info,
    Warning,
    Critical,
}

/// Sparse co-occurrence matrix (row, col) -> count.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SparseMatrix {
    pub entries: HashMap<(String, String), f64>,
}

/// Task dependency edge: `from` must complete before `to` can run.
#[derive(Debug, Clone)]
pub struct TaskDependency {
    pub from: String,
    pub to: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_ordering() {
        assert!(Priority::P0 < Priority::P1);
        assert!(Priority::P1 < Priority::P2);
        assert!(Priority::P2 < Priority::P3);
    }

    #[test]
    fn assess_load_normal() {
        let config = SchedulerConfig::default();
        assert_eq!(assess_load(0, &config), LoadLevel::Normal);
        assert_eq!(assess_load(500, &config), LoadLevel::Normal);
        assert_eq!(assess_load(1000, &config), LoadLevel::Normal);
    }

    #[test]
    fn assess_load_elevated() {
        let config = SchedulerConfig::default();
        assert_eq!(assess_load(1001, &config), LoadLevel::Elevated);
        assert_eq!(assess_load(5000, &config), LoadLevel::Elevated);
        assert_eq!(assess_load(10000, &config), LoadLevel::Elevated);
    }

    #[test]
    fn assess_load_critical() {
        let config = SchedulerConfig::default();
        assert_eq!(assess_load(10001, &config), LoadLevel::Critical);
        assert_eq!(assess_load(100000, &config), LoadLevel::Critical);
    }

    #[test]
    fn scheduler_config_defaults() {
        let config = SchedulerConfig::default();
        assert_eq!(config.worker_threads, 0);
        assert_eq!(config.p1_interval_seconds, 300);
        assert_eq!(config.p2_interval_seconds, 3600);
        assert_eq!(config.p3_interval_seconds, 86400);
        assert_eq!(config.backpressure_threshold, 1000);
        assert_eq!(config.critical_threshold, 10000);
    }

    #[test]
    fn resolved_worker_threads() {
        let mut config = SchedulerConfig::default();
        // 0 means auto-detect
        assert!(config.resolved_worker_threads() > 0);

        config.worker_threads = 8;
        assert_eq!(config.resolved_worker_threads(), 8);
    }

    #[test]
    fn interval_for_priority() {
        let config = SchedulerConfig::default();
        assert_eq!(config.interval_for(Priority::P0), Duration::ZERO);
        assert_eq!(config.interval_for(Priority::P1), Duration::from_secs(300));
        assert_eq!(config.interval_for(Priority::P2), Duration::from_secs(3600));
        assert_eq!(config.interval_for(Priority::P3), Duration::from_secs(86400));
    }
}
