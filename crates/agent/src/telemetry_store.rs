use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};
use tracing::info;

/// A single telemetry event from an agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryEvent {
    pub id: String,
    pub agent_name: String,
    pub timestamp: DateTime<Utc>,
    pub latency_ms: u64,
    pub tokens_used: u32,
    pub status: TelemetryStatus,
    pub provider: String,
    pub model: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_preview: Option<String>,
}

/// Outcome status of an agent execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TelemetryStatus {
    Success,
    Error,
    Timeout,
}

/// Aggregated statistics for a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryStats {
    pub agent_name: String,
    pub total_executions: usize,
    pub success_count: usize,
    pub error_count: usize,
    pub timeout_count: usize,
    pub avg_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub total_tokens: u64,
    pub error_rate: f64,
}

/// Append-only JSONL telemetry store — one file per agent.
pub struct TelemetryStore {
    dir: PathBuf,
}

impl TelemetryStore {
    /// Create a new telemetry store, ensuring the storage directory exists.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("telemetry");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create telemetry dir: {}", dir.display()))?;
        info!(path = %dir.display(), "telemetry store initialized");
        Ok(Self { dir })
    }

    /// Append a telemetry event to the agent's JSONL file.
    pub fn record(&self, event: TelemetryEvent) -> Result<()> {
        let path = self.agent_file(&event.agent_name);
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(&path)
            .with_context(|| format!("failed to open telemetry file: {}", path.display()))?;
        let mut line = serde_json::to_string(&event)?;
        line.push('\n');
        file.write_all(line.as_bytes())
            .with_context(|| format!("failed to write telemetry event: {}", path.display()))?;
        Ok(())
    }

    /// Get the most recent events for an agent, up to `limit`.
    pub fn events_for_agent(&self, agent_name: &str, limit: usize) -> Result<Vec<TelemetryEvent>> {
        let mut events = self.read_events(agent_name)?;
        // Return most recent first, capped at limit
        events.reverse();
        events.truncate(limit);
        Ok(events)
    }

    /// Get events for an agent within a time range (inclusive).
    pub fn events_in_range(
        &self,
        agent_name: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<TelemetryEvent>> {
        let events = self.read_events(agent_name)?;
        Ok(events
            .into_iter()
            .filter(|e| e.timestamp >= from && e.timestamp <= to)
            .collect())
    }

    /// Compute aggregated statistics for a single agent.
    pub fn stats_for_agent(&self, agent_name: &str) -> Result<TelemetryStats> {
        let events = self.read_events(agent_name)?;
        Ok(Self::compute_stats(agent_name, &events))
    }

    /// Compute statistics for ALL agents (one entry per agent).
    pub fn overview(&self) -> Result<Vec<TelemetryStats>> {
        let mut stats = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "jsonl") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let events = self.read_events(stem)?;
                    if !events.is_empty() {
                        stats.push(Self::compute_stats(stem, &events));
                    }
                }
            }
        }
        stats.sort_by(|a, b| b.total_executions.cmp(&a.total_executions));
        Ok(stats)
    }

    fn agent_file(&self, agent_name: &str) -> PathBuf {
        self.dir.join(format!("{}.jsonl", agent_name))
    }

    fn read_events(&self, agent_name: &str) -> Result<Vec<TelemetryEvent>> {
        let path = self.agent_file(agent_name);
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read telemetry file: {}", path.display()))?;
        let mut events = Vec::new();
        for (i, line) in data.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<TelemetryEvent>(line) {
                Ok(event) => events.push(event),
                Err(e) => {
                    tracing::warn!(
                        file = %path.display(),
                        line = i + 1,
                        error = %e,
                        "skipping corrupt telemetry line"
                    );
                }
            }
        }
        Ok(events)
    }

    fn compute_stats(agent_name: &str, events: &[TelemetryEvent]) -> TelemetryStats {
        let total = events.len();
        let success_count = events.iter().filter(|e| e.status == TelemetryStatus::Success).count();
        let error_count = events.iter().filter(|e| e.status == TelemetryStatus::Error).count();
        let timeout_count = events.iter().filter(|e| e.status == TelemetryStatus::Timeout).count();
        let total_tokens: u64 = events.iter().map(|e| e.tokens_used as u64).sum();

        let mut latencies: Vec<u64> = events.iter().map(|e| e.latency_ms).collect();
        latencies.sort_unstable();

        let avg_latency_ms = if total > 0 {
            latencies.iter().sum::<u64>() as f64 / total as f64
        } else {
            0.0
        };

        let p95_latency_ms = if total > 0 {
            let idx = ((0.95 * total as f64).ceil() as usize).saturating_sub(1).min(total - 1);
            latencies[idx] as f64
        } else {
            0.0
        };

        let error_rate = if total > 0 {
            error_count as f64 / total as f64
        } else {
            0.0
        };

        TelemetryStats {
            agent_name: agent_name.to_string(),
            total_executions: total,
            success_count,
            error_count,
            timeout_count,
            avg_latency_ms,
            p95_latency_ms,
            total_tokens,
            error_rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use uuid::Uuid;

    fn make_event(agent: &str, status: TelemetryStatus, latency_ms: u64) -> TelemetryEvent {
        TelemetryEvent {
            id: Uuid::new_v4().to_string(),
            agent_name: agent.to_string(),
            timestamp: Utc::now(),
            latency_ms,
            tokens_used: 100,
            status,
            provider: "anthropic".to_string(),
            model: "claude-sonnet-4-6".to_string(),
            error_message: None,
            task_preview: Some("test task".to_string()),
        }
    }

    #[test]
    fn record_and_read_events() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        store.record(make_event("agent-a", TelemetryStatus::Success, 120)).unwrap();
        store.record(make_event("agent-a", TelemetryStatus::Error, 500)).unwrap();
        store.record(make_event("agent-a", TelemetryStatus::Success, 80)).unwrap();

        let events = store.events_for_agent("agent-a", 10).unwrap();
        assert_eq!(events.len(), 3);
        // Most recent first
        assert_eq!(events[0].latency_ms, 80);
    }

    #[test]
    fn events_for_agent_respects_limit() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        for i in 0..10 {
            store.record(make_event("agent-b", TelemetryStatus::Success, i * 10)).unwrap();
        }

        let events = store.events_for_agent("agent-b", 3).unwrap();
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn events_in_range_filters_correctly() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        let t1 = Utc::now() - chrono::Duration::hours(2);
        let t2 = Utc::now() - chrono::Duration::hours(1);
        let t3 = Utc::now();

        let mut e1 = make_event("agent-c", TelemetryStatus::Success, 100);
        e1.timestamp = t1;
        let mut e2 = make_event("agent-c", TelemetryStatus::Success, 200);
        e2.timestamp = t2;
        let mut e3 = make_event("agent-c", TelemetryStatus::Success, 300);
        e3.timestamp = t3;

        store.record(e1).unwrap();
        store.record(e2).unwrap();
        store.record(e3).unwrap();

        // Query range that only includes e2
        let from = t1 + chrono::Duration::minutes(30);
        let to = t2 + chrono::Duration::minutes(30);
        let events = store.events_in_range("agent-c", from, to).unwrap();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].latency_ms, 200);
    }

    #[test]
    fn stats_computation() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        store.record(make_event("agent-d", TelemetryStatus::Success, 100)).unwrap();
        store.record(make_event("agent-d", TelemetryStatus::Success, 200)).unwrap();
        store.record(make_event("agent-d", TelemetryStatus::Error, 500)).unwrap();
        store.record(make_event("agent-d", TelemetryStatus::Timeout, 5000)).unwrap();

        let stats = store.stats_for_agent("agent-d").unwrap();
        assert_eq!(stats.total_executions, 4);
        assert_eq!(stats.success_count, 2);
        assert_eq!(stats.error_count, 1);
        assert_eq!(stats.timeout_count, 1);
        assert_eq!(stats.total_tokens, 400);
        assert!((stats.error_rate - 0.25).abs() < f64::EPSILON);
        assert!((stats.avg_latency_ms - 1450.0).abs() < f64::EPSILON);
    }

    #[test]
    fn overview_lists_all_agents() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        store.record(make_event("alpha", TelemetryStatus::Success, 100)).unwrap();
        store.record(make_event("alpha", TelemetryStatus::Success, 200)).unwrap();
        store.record(make_event("beta", TelemetryStatus::Error, 300)).unwrap();

        let overview = store.overview().unwrap();
        assert_eq!(overview.len(), 2);
        // Sorted by total_executions descending
        assert_eq!(overview[0].agent_name, "alpha");
        assert_eq!(overview[0].total_executions, 2);
        assert_eq!(overview[1].agent_name, "beta");
        assert_eq!(overview[1].total_executions, 1);
    }

    #[test]
    fn empty_agent_returns_empty() {
        let tmp = TempDir::new().unwrap();
        let store = TelemetryStore::new(tmp.path()).unwrap();

        let events = store.events_for_agent("nonexistent", 10).unwrap();
        assert!(events.is_empty());

        let stats = store.stats_for_agent("nonexistent").unwrap();
        assert_eq!(stats.total_executions, 0);
        assert_eq!(stats.avg_latency_ms, 0.0);
    }
}
