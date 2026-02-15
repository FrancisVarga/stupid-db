use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// Where a query originated — user-initiated or internal schema refresh.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuerySource {
    UserQuery,
    SchemaRefreshDatabases,
    SchemaRefreshTables,
    SchemaRefreshDescribe,
}

/// Terminal state of an Athena query execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QueryOutcome {
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
}

// ---------------------------------------------------------------------------
// Core log entry
// ---------------------------------------------------------------------------

/// A single audited Athena query execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaQueryLogEntry {
    /// Monotonic counter scoped to the connection.
    pub entry_id: u64,
    pub connection_id: String,
    /// `None` when `StartQueryExecution` itself failed before returning an id.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_execution_id: Option<String>,
    pub source: QuerySource,
    pub sql: String,
    pub database: String,
    pub workgroup: String,
    pub outcome: QueryOutcome,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
    pub data_scanned_bytes: i64,
    pub engine_execution_time_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_rows: Option<u64>,
    pub estimated_cost_usd: f64,
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub wall_clock_ms: i64,
}

// ---------------------------------------------------------------------------
// REST query parameters
// ---------------------------------------------------------------------------

/// Query-string parameters for the `GET /athena-connections/{id}/query-log`
/// endpoint.
#[derive(Debug, Deserialize)]
pub struct QueryLogParams {
    pub source: Option<QuerySource>,
    pub outcome: Option<QueryOutcome>,
    /// ISO 8601 lower bound (inclusive).
    pub since: Option<String>,
    /// ISO 8601 upper bound (exclusive).
    pub until: Option<String>,
    /// Maximum entries to return (default 100).
    pub limit: Option<u32>,
    /// Case-insensitive substring match against the SQL text.
    pub sql_contains: Option<String>,
}

// ---------------------------------------------------------------------------
// Summary / response types
// ---------------------------------------------------------------------------

/// Cost breakdown for a single calendar day.
#[derive(Debug, Clone, Serialize)]
pub struct DailyCostSummary {
    /// `YYYY-MM-DD` formatted date.
    pub date: String,
    pub query_count: u64,
    pub total_bytes_scanned: i64,
    pub total_cost_usd: f64,
    /// Cost keyed by `QuerySource` variant (snake_case string).
    pub by_source: HashMap<String, f64>,
}

/// Aggregated statistics across all returned log entries.
#[derive(Debug, Serialize)]
pub struct QueryLogSummary {
    pub total_queries: u64,
    pub total_bytes_scanned: i64,
    pub total_cost_usd: f64,
    pub daily: Vec<DailyCostSummary>,
}

/// Top-level API response wrapping entries and their summary.
#[derive(Debug, Serialize)]
pub struct QueryLogResponse {
    pub connection_id: String,
    pub entries: Vec<AthenaQueryLogEntry>,
    pub summary: QueryLogSummary,
}

// ---------------------------------------------------------------------------
// Cost calculation
// ---------------------------------------------------------------------------

/// Athena pricing: $5.00 per TB scanned, 10 MB minimum for data-scanning
/// queries. DDL / metadata queries (`data_scanned_bytes == 0`) are free.
const ATHENA_COST_PER_BYTE: f64 = 5.0 / (1024.0 * 1024.0 * 1024.0 * 1024.0);
const ATHENA_MIN_SCAN_BYTES: i64 = 10 * 1024 * 1024; // 10 MB

/// Return the estimated USD cost for an Athena query that scanned
/// `data_scanned_bytes` bytes.
pub fn calculate_query_cost(data_scanned_bytes: i64) -> f64 {
    if data_scanned_bytes == 0 {
        return 0.0;
    }
    let billable = data_scanned_bytes.max(ATHENA_MIN_SCAN_BYTES);
    billable as f64 * ATHENA_COST_PER_BYTE
}

// ---------------------------------------------------------------------------
// Persistent query log store
// ---------------------------------------------------------------------------

/// Per-connection Athena query audit log with file-backed persistence.
///
/// In-memory ring buffer per connection (1000 entries FIFO) with immediate
/// JSON persistence to `{data_dir}/athena-query-log-{connection_id}.json`.
pub struct AthenaQueryLog {
    data_dir: PathBuf,
    entries: RwLock<HashMap<String, VecDeque<AthenaQueryLogEntry>>>,
    counters: RwLock<HashMap<String, u64>>,
    max_entries_per_connection: usize,
}

impl AthenaQueryLog {
    /// Create a new query log store.
    pub fn new(data_dir: &PathBuf) -> Self {
        Self {
            data_dir: data_dir.clone(),
            entries: RwLock::new(HashMap::new()),
            counters: RwLock::new(HashMap::new()),
            max_entries_per_connection: 1000,
        }
    }

    fn log_path(&self, connection_id: &str) -> PathBuf {
        self.data_dir
            .join(format!("athena-query-log-{}.json", connection_id))
    }

    /// Load existing log from disk for a connection (lazy, on first access).
    fn ensure_loaded(&self, connection_id: &str) {
        // Check if already loaded
        {
            let entries = self.entries.read().expect("query_log lock poisoned");
            if entries.contains_key(connection_id) {
                return;
            }
        }

        // Try to load from disk
        let path = self.log_path(connection_id);
        let loaded: VecDeque<AthenaQueryLogEntry> = if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(e) => {
                    tracing::warn!("Failed to read query log {}: {}", path.display(), e);
                    VecDeque::new()
                }
            }
        } else {
            VecDeque::new()
        };

        // Determine next counter value
        let max_id = loaded.iter().map(|e| e.entry_id).max().unwrap_or(0);

        let mut entries = self.entries.write().expect("query_log lock poisoned");
        let mut counters = self
            .counters
            .write()
            .expect("query_log counter lock poisoned");
        entries
            .entry(connection_id.to_string())
            .or_insert(loaded);
        counters
            .entry(connection_id.to_string())
            .or_insert(max_id + 1);
    }

    /// Persist current entries for a connection to disk.
    fn persist(&self, connection_id: &str) {
        let entries = self.entries.read().expect("query_log lock poisoned");
        if let Some(deque) = entries.get(connection_id) {
            let path = self.log_path(connection_id);
            match serde_json::to_string_pretty(deque) {
                Ok(json) => {
                    if let Err(e) = std::fs::write(&path, json) {
                        tracing::warn!(
                            "Failed to persist query log to {}: {}",
                            path.display(),
                            e
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to serialize query log: {}", e);
                }
            }
        }
    }

    /// Append a completed query entry. Assigns entry_id and persists to disk
    /// immediately.
    pub fn append(&self, mut entry: AthenaQueryLogEntry) {
        let connection_id = entry.connection_id.clone();
        self.ensure_loaded(&connection_id);

        // Assign entry_id
        {
            let mut counters = self
                .counters
                .write()
                .expect("query_log counter lock poisoned");
            let counter = counters.entry(connection_id.clone()).or_insert(1);
            entry.entry_id = *counter;
            *counter += 1;
        }

        // Append with FIFO eviction
        {
            let mut entries = self.entries.write().expect("query_log lock poisoned");
            let deque = entries
                .entry(connection_id.clone())
                .or_insert_with(VecDeque::new);
            deque.push_back(entry);
            while deque.len() > self.max_entries_per_connection {
                deque.pop_front();
            }
        }

        self.persist(&connection_id);
    }

    /// Query log entries with filters, newest first.
    pub fn query(
        &self,
        connection_id: &str,
        params: &QueryLogParams,
    ) -> Vec<AthenaQueryLogEntry> {
        self.ensure_loaded(connection_id);

        let entries = self.entries.read().expect("query_log lock poisoned");
        let Some(deque) = entries.get(connection_id) else {
            return Vec::new();
        };

        let since: Option<DateTime<Utc>> = params
            .since
            .as_ref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let until: Option<DateTime<Utc>> = params
            .until
            .as_ref()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok());
        let limit = params.limit.unwrap_or(100) as usize;

        deque
            .iter()
            .rev() // newest first
            .filter(|e| params.source.as_ref().map_or(true, |s| &e.source == s))
            .filter(|e| {
                params
                    .outcome
                    .as_ref()
                    .map_or(true, |o| &e.outcome == o)
            })
            .filter(|e| since.map_or(true, |s| e.started_at >= s))
            .filter(|e| until.map_or(true, |u| e.started_at < u))
            .filter(|e| {
                params.sql_contains.as_ref().map_or(true, |needle| {
                    e.sql.to_lowercase().contains(&needle.to_lowercase())
                })
            })
            .take(limit)
            .cloned()
            .collect()
    }

    /// Get cumulative + daily cost summary for a connection.
    pub fn summary(&self, connection_id: &str) -> QueryLogSummary {
        self.ensure_loaded(connection_id);

        let entries = self.entries.read().expect("query_log lock poisoned");
        let Some(deque) = entries.get(connection_id) else {
            return QueryLogSummary {
                total_queries: 0,
                total_bytes_scanned: 0,
                total_cost_usd: 0.0,
                daily: Vec::new(),
            };
        };

        let mut total_bytes: i64 = 0;
        let mut total_cost: f64 = 0.0;
        // day_str -> (count, bytes, cost, source_costs)
        let mut daily_map: HashMap<String, (u64, i64, f64, HashMap<String, f64>)> =
            HashMap::new();

        for entry in deque.iter() {
            total_bytes += entry.data_scanned_bytes;
            total_cost += entry.estimated_cost_usd;

            let day = entry.started_at.format("%Y-%m-%d").to_string();
            let daily = daily_map
                .entry(day)
                .or_insert_with(|| (0, 0, 0.0, HashMap::new()));
            daily.0 += 1;
            daily.1 += entry.data_scanned_bytes;
            daily.2 += entry.estimated_cost_usd;

            // Use serde serialization name for source key
            let source_key = serde_json::to_value(&entry.source)
                .ok()
                .and_then(|v| v.as_str().map(|s| s.to_string()))
                .unwrap_or_else(|| format!("{:?}", entry.source));
            *daily.3.entry(source_key).or_insert(0.0) += entry.estimated_cost_usd;
        }

        let mut daily_summaries: Vec<DailyCostSummary> = daily_map
            .into_iter()
            .map(|(date, (count, bytes, cost, by_source))| DailyCostSummary {
                date,
                query_count: count,
                total_bytes_scanned: bytes,
                total_cost_usd: cost,
                by_source,
            })
            .collect();
        daily_summaries.sort_by(|a, b| b.date.cmp(&a.date)); // newest first

        QueryLogSummary {
            total_queries: deque.len() as u64,
            total_bytes_scanned: total_bytes,
            total_cost_usd: total_cost,
            daily: daily_summaries,
        }
    }

    /// Delete all log entries for a connection (in-memory + disk file).
    pub fn clear(&self, connection_id: &str) {
        {
            let mut entries = self.entries.write().expect("query_log lock poisoned");
            entries.remove(connection_id);
        }
        {
            let mut counters = self
                .counters
                .write()
                .expect("query_log counter lock poisoned");
            counters.remove(connection_id);
        }
        let path = self.log_path(connection_id);
        if path.exists() {
            if let Err(e) = std::fs::remove_file(&path) {
                tracing::warn!(
                    "Failed to remove query log file {}: {}",
                    path.display(),
                    e
                );
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zero_scan_is_free() {
        assert_eq!(calculate_query_cost(0), 0.0);
    }

    #[test]
    fn test_minimum_billing() {
        // Anything below 10 MB should bill as 10 MB.
        let cost_1byte = calculate_query_cost(1);
        let cost_10mb = calculate_query_cost(10 * 1024 * 1024);
        assert_eq!(cost_1byte, cost_10mb);
    }

    #[test]
    fn test_1tb_costs_5_dollars() {
        let one_tb: i64 = 1024 * 1024 * 1024 * 1024;
        let cost = calculate_query_cost(one_tb);
        assert!((cost - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_above_minimum() {
        let one_gb: i64 = 1024 * 1024 * 1024;
        let cost = calculate_query_cost(one_gb);
        let expected = one_gb as f64 * ATHENA_COST_PER_BYTE;
        assert!((cost - expected).abs() < 0.0001);
    }

    #[test]
    fn test_append_and_query() {
        let dir = std::env::temp_dir().join(format!("athena_log_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = AthenaQueryLog::new(&dir);

        let entry = AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "test-conn".into(),
            query_execution_id: Some("qid-1".into()),
            source: QuerySource::UserQuery,
            sql: "SELECT 1".into(),
            database: "default".into(),
            workgroup: "primary".into(),
            outcome: QueryOutcome::Succeeded,
            error_message: None,
            data_scanned_bytes: 1024 * 1024 * 100, // 100MB
            engine_execution_time_ms: 500,
            total_rows: Some(1),
            estimated_cost_usd: calculate_query_cost(1024 * 1024 * 100),
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 500,
        };

        log.append(entry);

        let params = QueryLogParams {
            source: None,
            outcome: None,
            since: None,
            until: None,
            limit: None,
            sql_contains: None,
        };
        let results = log.query("test-conn", &params);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].entry_id, 1);
        assert_eq!(results[0].sql, "SELECT 1");

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_fifo_eviction() {
        let dir =
            std::env::temp_dir().join(format!("athena_log_evict_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut log = AthenaQueryLog::new(&dir);
        log.max_entries_per_connection = 3;

        for i in 0..5 {
            log.append(AthenaQueryLogEntry {
                entry_id: 0,
                connection_id: "conn".into(),
                query_execution_id: Some(format!("q-{}", i)),
                source: QuerySource::UserQuery,
                sql: format!("SELECT {}", i),
                database: "db".into(),
                workgroup: "wg".into(),
                outcome: QueryOutcome::Succeeded,
                error_message: None,
                data_scanned_bytes: 0,
                engine_execution_time_ms: 0,
                total_rows: None,
                estimated_cost_usd: 0.0,
                started_at: Utc::now(),
                completed_at: Utc::now(),
                wall_clock_ms: 0,
            });
        }

        let params = QueryLogParams {
            source: None,
            outcome: None,
            since: None,
            until: None,
            limit: None,
            sql_contains: None,
        };
        let results = log.query("conn", &params);
        assert_eq!(results.len(), 3);
        // Oldest (0, 1) should be evicted; newest first in results
        assert_eq!(results[0].sql, "SELECT 4");
        assert_eq!(results[2].sql, "SELECT 2");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_persistence_round_trip() {
        let dir =
            std::env::temp_dir().join(format!("athena_log_persist_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        // Write with one instance
        {
            let log = AthenaQueryLog::new(&dir);
            log.append(AthenaQueryLogEntry {
                entry_id: 0,
                connection_id: "persist-conn".into(),
                query_execution_id: Some("q-persist".into()),
                source: QuerySource::SchemaRefreshDatabases,
                sql: "SHOW DATABASES".into(),
                database: "default".into(),
                workgroup: "primary".into(),
                outcome: QueryOutcome::Succeeded,
                error_message: None,
                data_scanned_bytes: 0,
                engine_execution_time_ms: 100,
                total_rows: Some(5),
                estimated_cost_usd: 0.0,
                started_at: Utc::now(),
                completed_at: Utc::now(),
                wall_clock_ms: 100,
            });
        }

        // Read with new instance (simulates server restart)
        {
            let log = AthenaQueryLog::new(&dir);
            let params = QueryLogParams {
                source: None,
                outcome: None,
                since: None,
                until: None,
                limit: None,
                sql_contains: None,
            };
            let results = log.query("persist-conn", &params);
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].sql, "SHOW DATABASES");
            assert_eq!(results[0].entry_id, 1);
        }

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_summary_daily_aggregates() {
        let dir =
            std::env::temp_dir().join(format!("athena_log_summary_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = AthenaQueryLog::new(&dir);

        let one_gb: i64 = 1024 * 1024 * 1024;

        for i in 0..3 {
            log.append(AthenaQueryLogEntry {
                entry_id: 0,
                connection_id: "sum-conn".into(),
                query_execution_id: Some(format!("q-{}", i)),
                source: if i == 0 {
                    QuerySource::UserQuery
                } else {
                    QuerySource::SchemaRefreshTables
                },
                sql: format!("query {}", i),
                database: "db".into(),
                workgroup: "wg".into(),
                outcome: QueryOutcome::Succeeded,
                error_message: None,
                data_scanned_bytes: one_gb,
                engine_execution_time_ms: 1000,
                total_rows: Some(100),
                estimated_cost_usd: calculate_query_cost(one_gb),
                started_at: Utc::now(),
                completed_at: Utc::now(),
                wall_clock_ms: 1000,
            });
        }

        let summary = log.summary("sum-conn");
        assert_eq!(summary.total_queries, 3);
        assert_eq!(summary.total_bytes_scanned, 3 * one_gb);
        assert!(summary.total_cost_usd > 0.0);
        assert_eq!(summary.daily.len(), 1); // All same day
        assert_eq!(summary.daily[0].query_count, 3);

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_clear_removes_entries_and_file() {
        let dir =
            std::env::temp_dir().join(format!("athena_log_clear_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = AthenaQueryLog::new(&dir);

        log.append(AthenaQueryLogEntry {
            entry_id: 0,
            connection_id: "clear-conn".into(),
            query_execution_id: None,
            source: QuerySource::UserQuery,
            sql: "SELECT 1".into(),
            database: "db".into(),
            workgroup: "wg".into(),
            outcome: QueryOutcome::Failed,
            error_message: Some("test error".into()),
            data_scanned_bytes: 0,
            engine_execution_time_ms: 0,
            total_rows: None,
            estimated_cost_usd: 0.0,
            started_at: Utc::now(),
            completed_at: Utc::now(),
            wall_clock_ms: 0,
        });

        // File should exist
        assert!(log.log_path("clear-conn").exists());

        log.clear("clear-conn");

        // File should be deleted
        assert!(!log.log_path("clear-conn").exists());

        // Query should return empty
        let params = QueryLogParams {
            source: None,
            outcome: None,
            since: None,
            until: None,
            limit: None,
            sql_contains: None,
        };
        let results = log.query("clear-conn", &params);
        assert!(results.is_empty());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn test_filter_by_source() {
        let dir =
            std::env::temp_dir().join(format!("athena_log_filter_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let log = AthenaQueryLog::new(&dir);

        for source in [
            QuerySource::UserQuery,
            QuerySource::SchemaRefreshDatabases,
            QuerySource::UserQuery,
        ] {
            log.append(AthenaQueryLogEntry {
                entry_id: 0,
                connection_id: "filter-conn".into(),
                query_execution_id: None,
                source,
                sql: "test".into(),
                database: "db".into(),
                workgroup: "wg".into(),
                outcome: QueryOutcome::Succeeded,
                error_message: None,
                data_scanned_bytes: 0,
                engine_execution_time_ms: 0,
                total_rows: None,
                estimated_cost_usd: 0.0,
                started_at: Utc::now(),
                completed_at: Utc::now(),
                wall_clock_ms: 0,
            });
        }

        let params = QueryLogParams {
            source: Some(QuerySource::UserQuery),
            outcome: None,
            since: None,
            until: None,
            limit: None,
            sql_contains: None,
        };
        let results = log.query("filter-conn", &params);
        assert_eq!(results.len(), 2);

        std::fs::remove_dir_all(&dir).ok();
    }
}
