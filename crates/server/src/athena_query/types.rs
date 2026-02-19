//! Athena query result types.

/// Result from execute_and_wait_with_stats, including execution metadata.
pub struct QueryExecutionResult {
    pub query_execution_id: String,
    #[allow(dead_code)] // available for callers that need column metadata
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub data_scanned_bytes: i64,
    pub engine_execution_time_ms: i64,
}
