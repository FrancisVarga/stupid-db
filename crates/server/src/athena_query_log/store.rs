use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::RwLock;

use chrono::{DateTime, Utc};

use super::types::{
    AthenaQueryLogEntry, DailyCostSummary, QueryLogParams, QueryLogSummary,
};

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
    pub(crate) max_entries_per_connection: usize,
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

    pub(crate) fn log_path(&self, connection_id: &str) -> PathBuf {
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
