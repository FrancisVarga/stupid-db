//! AWS Athena query execution client.
//!
//! Provides [`AthenaClient`] for executing SQL queries against AWS Athena,
//! with exponential-backoff polling, timeout enforcement, scan-limit checks,
//! and structured result parsing into [`AthenaQueryResult`].

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aws_config::BehaviorVersion;
use aws_sdk_athena::types::QueryExecutionState;
use tracing::{debug, error, info, warn};

use crate::config::AthenaConfig;
use crate::result::{AthenaColumn, AthenaQueryResult, QueryMetadata};

// ---------------------------------------------------------------------------
// Error
// ---------------------------------------------------------------------------

/// Errors that can occur during Athena operations.
#[derive(Debug, thiserror::Error)]
pub enum AthenaError {
    /// Athena integration is not enabled in configuration.
    #[error("Athena is not enabled in config")]
    NotEnabled,

    /// The query execution failed on the Athena side.
    #[error("Query {query_id} failed: {reason}")]
    QueryFailed { query_id: String, reason: String },

    /// The query was cancelled (either by the user or by Athena).
    #[error("Query {query_id} was cancelled")]
    QueryCancelled { query_id: String },

    /// The query exceeded the configured timeout.
    #[error("Query {query_id} timed out after {seconds}s")]
    QueryTimeout { query_id: String, seconds: u32 },

    /// The query scanned more bytes than the configured limit.
    #[error("Scan limit exceeded: {bytes_scanned} bytes scanned, limit is {limit} bytes")]
    ScanLimitExceeded { bytes_scanned: u64, limit: u64 },

    /// An AWS SDK error (stringified).
    #[error("AWS SDK error: {0}")]
    AwsSdk(String),

    /// Failed to parse Athena result data.
    #[error("Parse error: {0}")]
    ParseError(String),
}

// ---------------------------------------------------------------------------
// Client
// ---------------------------------------------------------------------------

/// Client for executing queries against AWS Athena.
///
/// Wraps the AWS SDK Athena client and adds:
/// - Exponential-backoff polling with jitter
/// - Timeout enforcement with automatic cancellation
/// - Scan-limit checking (post-execution)
/// - Structured result parsing into [`AthenaQueryResult`]
pub struct AthenaClient {
    config: AthenaConfig,
    athena_client: aws_sdk_athena::Client,
}

impl AthenaClient {
    /// Create a new [`AthenaClient`] from the given configuration.
    ///
    /// Returns [`AthenaError::NotEnabled`] if the config has Athena disabled.
    /// The AWS SDK config is loaded using the region specified in `config`.
    pub async fn new(config: AthenaConfig) -> Result<Self, AthenaError> {
        if !config.enabled {
            return Err(AthenaError::NotEnabled);
        }

        let region = aws_sdk_athena::config::Region::new(config.region.clone());
        let aws_cfg = aws_config::defaults(BehaviorVersion::latest())
            .region(region)
            .load()
            .await;

        let athena_client = aws_sdk_athena::Client::new(&aws_cfg);

        info!(
            region = %config.region,
            database = %config.database,
            workgroup = %config.workgroup,
            "AthenaClient initialised"
        );

        Ok(Self {
            config,
            athena_client,
        })
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    /// Execute a SQL query against Athena and return the parsed results.
    ///
    /// This performs the full lifecycle:
    /// 1. Start query execution
    /// 2. Poll until completion (with exponential backoff)
    /// 3. Fetch and parse results on success
    pub async fn execute_query(&self, sql: &str) -> Result<AthenaQueryResult, AthenaError> {
        info!(sql = %sql, "Starting Athena query");

        // 1. Start query execution
        let start_resp = self
            .athena_client
            .start_query_execution()
            .query_string(sql)
            .query_execution_context({
                let mut ctx = aws_sdk_athena::types::QueryExecutionContext::builder();
                if !self.config.database.is_empty() {
                    ctx = ctx.database(&self.config.database);
                }
                ctx.build()
            })
            .result_configuration(
                aws_sdk_athena::types::ResultConfiguration::builder()
                    .output_location(&self.config.output_location)
                    .build(),
            )
            .work_group(&self.config.workgroup)
            .send()
            .await
            .map_err(|e| AthenaError::AwsSdk(e.to_string()))?;

        let query_id = start_resp
            .query_execution_id()
            .ok_or_else(|| AthenaError::AwsSdk("No query execution ID returned".into()))?
            .to_string();

        info!(query_id = %query_id, "Query execution started");

        // 2. Poll until complete
        let query_execution = self.poll_until_complete(&query_id).await?;

        // 3. Build metadata
        let metadata = Self::extract_metadata(&query_id, &query_execution);

        // 4. Fetch and parse results
        let results_output = self
            .athena_client
            .get_query_results()
            .query_execution_id(&query_id)
            .send()
            .await
            .map_err(|e| AthenaError::AwsSdk(e.to_string()))?;

        self.parse_results(&results_output, metadata)
    }

    /// Execute a SQL query and check that bytes scanned does not exceed `max_scan_bytes`.
    ///
    /// Because Athena does not support pre-execution scan estimation, this check
    /// happens **after** the query completes. If the limit is exceeded the result
    /// is still returned but an error is logged and [`AthenaError::ScanLimitExceeded`]
    /// is returned.
    pub async fn execute_query_with_limit(
        &self,
        sql: &str,
        max_scan_bytes: u64,
    ) -> Result<AthenaQueryResult, AthenaError> {
        let result = self.execute_query(sql).await?;

        if result.metadata.bytes_scanned > max_scan_bytes {
            warn!(
                bytes_scanned = result.metadata.bytes_scanned,
                limit = max_scan_bytes,
                query_id = %result.metadata.query_id,
                "Query exceeded scan limit"
            );
            return Err(AthenaError::ScanLimitExceeded {
                bytes_scanned: result.metadata.bytes_scanned,
                limit: max_scan_bytes,
            });
        }

        Ok(result)
    }

    /// Cancel a running Athena query.
    pub async fn cancel_query(&self, query_id: &str) -> Result<(), AthenaError> {
        info!(query_id = %query_id, "Cancelling query");

        self.athena_client
            .stop_query_execution()
            .query_execution_id(query_id)
            .send()
            .await
            .map_err(|e| AthenaError::AwsSdk(e.to_string()))?;

        info!(query_id = %query_id, "Query cancellation requested");
        Ok(())
    }

    /// Get the current status / metadata for an existing query execution.
    pub async fn get_query_status(
        &self,
        query_id: &str,
    ) -> Result<QueryMetadata, AthenaError> {
        let resp = self
            .athena_client
            .get_query_execution()
            .query_execution_id(query_id)
            .send()
            .await
            .map_err(|e| AthenaError::AwsSdk(e.to_string()))?;

        let qe = resp
            .query_execution()
            .ok_or_else(|| AthenaError::AwsSdk("No query execution in response".into()))?;

        Ok(Self::extract_metadata(query_id, qe))
    }

    // -----------------------------------------------------------------------
    // Private helpers
    // -----------------------------------------------------------------------

    /// Poll [`GetQueryExecution`] with exponential backoff until the query
    /// reaches a terminal state (SUCCEEDED, FAILED, CANCELLED) or the
    /// configured timeout is exceeded.
    async fn poll_until_complete(
        &self,
        query_id: &str,
    ) -> Result<aws_sdk_athena::types::QueryExecution, AthenaError> {
        let start = Instant::now();
        let timeout = Duration::from_secs(self.config.timeout_seconds as u64);

        let initial_delay_ms: u64 = 200;
        let max_delay_ms: u64 = 2000;
        let backoff_factor: f64 = 1.5;

        let mut delay_ms = initial_delay_ms;

        loop {
            let resp = self
                .athena_client
                .get_query_execution()
                .query_execution_id(query_id)
                .send()
                .await
                .map_err(|e| AthenaError::AwsSdk(e.to_string()))?;

            let qe = resp
                .query_execution()
                .ok_or_else(|| {
                    AthenaError::AwsSdk("No query execution in response".into())
                })?
                .clone();

            let state = qe
                .status()
                .and_then(|s| s.state())
                .cloned()
                .unwrap_or(QueryExecutionState::Queued);

            debug!(
                query_id = %query_id,
                state = ?state,
                elapsed_ms = start.elapsed().as_millis() as u64,
                "Polling query status"
            );

            match state {
                QueryExecutionState::Succeeded => return Ok(qe),

                QueryExecutionState::Failed => {
                    let reason = qe
                        .status()
                        .and_then(|s| s.state_change_reason())
                        .unwrap_or("unknown")
                        .to_string();

                    error!(query_id = %query_id, reason = %reason, "Query failed");
                    return Err(AthenaError::QueryFailed {
                        query_id: query_id.to_string(),
                        reason,
                    });
                }

                QueryExecutionState::Cancelled => {
                    warn!(query_id = %query_id, "Query was cancelled");
                    return Err(AthenaError::QueryCancelled {
                        query_id: query_id.to_string(),
                    });
                }

                // Queued | Running | unknown future variant
                _ => {}
            }

            // Check timeout
            if start.elapsed() > timeout {
                warn!(
                    query_id = %query_id,
                    timeout_seconds = self.config.timeout_seconds,
                    "Query timed out, cancelling"
                );
                // Best-effort cancel — ignore errors from the cancel itself
                let _ = self.cancel_query(query_id).await;
                return Err(AthenaError::QueryTimeout {
                    query_id: query_id.to_string(),
                    seconds: self.config.timeout_seconds,
                });
            }

            // Compute jitter without rand: use nanosecond fraction of current time
            let jitter_ms = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .subsec_nanos()
                % 100;

            let sleep_ms = delay_ms + jitter_ms as u64;
            tokio::time::sleep(Duration::from_millis(sleep_ms)).await;

            // Advance backoff
            delay_ms = ((delay_ms as f64 * backoff_factor) as u64).min(max_delay_ms);
        }
    }

    /// Parse the raw SDK [`GetQueryResultsOutput`] into our [`AthenaQueryResult`].
    ///
    /// Athena returns column metadata in `ResultSetMetadata` and data rows in
    /// `ResultSet.Rows`. When `UpdateCount` is `None` the first row duplicates
    /// the column headers and must be skipped.
    fn parse_results(
        &self,
        output: &aws_sdk_athena::operation::get_query_results::GetQueryResultsOutput,
        metadata: QueryMetadata,
    ) -> Result<AthenaQueryResult, AthenaError> {
        let result_set = output
            .result_set()
            .ok_or_else(|| AthenaError::ParseError("No ResultSet in response".into()))?;

        // -- Columns ---------------------------------------------------------
        let columns: Vec<AthenaColumn> = result_set
            .result_set_metadata()
            .map(|meta| {
                meta.column_info()
                    .iter()
                    .map(|ci| AthenaColumn {
                        name: ci.name().to_string(),
                        data_type: ci.r#type().to_string(),
                    })
                    .collect()
            })
            .unwrap_or_default();

        // -- Rows ------------------------------------------------------------
        let raw_rows = result_set.rows();

        // When UpdateCount is None, the first row is a header echo — skip it.
        let skip = if output.update_count().is_none() && !raw_rows.is_empty() {
            1
        } else {
            0
        };

        let rows: Vec<Vec<Option<String>>> = raw_rows
            .iter()
            .skip(skip)
            .map(|row| {
                row.data()
                    .iter()
                    .map(|datum| datum.var_char_value().map(|v| v.to_string()))
                    .collect()
            })
            .collect();

        debug!(
            columns = columns.len(),
            rows = rows.len(),
            query_id = %metadata.query_id,
            "Parsed Athena results"
        );

        Ok(AthenaQueryResult {
            columns,
            rows,
            metadata,
        })
    }

    /// Extract [`QueryMetadata`] from an SDK [`QueryExecution`].
    fn extract_metadata(
        query_id: &str,
        qe: &aws_sdk_athena::types::QueryExecution,
    ) -> QueryMetadata {
        let stats = qe.statistics();
        let status = qe.status();

        QueryMetadata {
            query_id: query_id.to_string(),
            bytes_scanned: stats
                .and_then(|s| s.data_scanned_in_bytes())
                .unwrap_or(0) as u64,
            execution_time_ms: stats
                .and_then(|s| s.engine_execution_time_in_millis())
                .unwrap_or(0) as u64,
            state: status
                .and_then(|s| s.state())
                .map(|s| format!("{:?}", s))
                .unwrap_or_else(|| "UNKNOWN".to_string()),
            output_location: qe
                .result_configuration()
                .and_then(|rc| rc.output_location())
                .map(|s| s.to_string()),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — parsing logic only, no AWS calls
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jitter_is_bounded() {
        // The jitter calculation should always produce a value in [0, 100).
        for _ in 0..1000 {
            let jitter = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .subsec_nanos()
                % 100;
            assert!(jitter < 100);
        }
    }

    #[test]
    fn error_display_messages() {
        let err = AthenaError::NotEnabled;
        assert_eq!(err.to_string(), "Athena is not enabled in config");

        let err = AthenaError::QueryFailed {
            query_id: "abc-123".into(),
            reason: "syntax error".into(),
        };
        assert!(err.to_string().contains("abc-123"));
        assert!(err.to_string().contains("syntax error"));

        let err = AthenaError::QueryTimeout {
            query_id: "t-1".into(),
            seconds: 60,
        };
        assert!(err.to_string().contains("60s"));

        let err = AthenaError::ScanLimitExceeded {
            bytes_scanned: 1_000_000,
            limit: 500_000,
        };
        assert!(err.to_string().contains("1000000"));
        assert!(err.to_string().contains("500000"));
    }
}
