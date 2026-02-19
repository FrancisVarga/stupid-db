use std::collections::HashMap;

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
