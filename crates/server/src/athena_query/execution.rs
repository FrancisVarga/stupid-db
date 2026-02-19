//! Athena client construction and query execution utilities.
//!
//! Provides a reusable Athena client builder from decrypted credentials
//! and helper functions for executing queries with polling.

use aws_sdk_athena::Client as AthenaClient;
use aws_sdk_athena::config::{BehaviorVersion, Region};
use aws_credential_types::Credentials;

use crate::athena_connections::AthenaConnectionCredentials;

use super::types::QueryExecutionResult;

/// Build an Athena client from decrypted connection credentials.
pub async fn build_athena_client(creds: &AthenaConnectionCredentials) -> AthenaClient {
    let aws_creds = Credentials::new(
        &creds.access_key_id,
        &creds.secret_access_key,
        if creds.session_token.is_empty() {
            None
        } else {
            Some(creds.session_token.clone())
        },
        None, // expiry
        "stupid-db-athena",
    );

    let mut config_builder = aws_sdk_athena::Config::builder()
        .behavior_version(BehaviorVersion::latest())
        .region(Region::new(creds.region.clone()))
        .credentials_provider(aws_creds);

    if let Some(ref endpoint) = creds.endpoint_url {
        if !endpoint.is_empty() {
            config_builder = config_builder.endpoint_url(endpoint);
        }
    }

    AthenaClient::from_conf(config_builder.build())
}

/// Execute an Athena SQL query and return the query execution ID.
pub async fn start_query(
    client: &AthenaClient,
    sql: &str,
    catalog: &str,
    database: &str,
    workgroup: &str,
    output_location: &str,
) -> anyhow::Result<String> {
    let result = client
        .start_query_execution()
        .query_string(sql)
        .query_execution_context({
            let mut ctx = aws_sdk_athena::types::QueryExecutionContext::builder();
            if !catalog.is_empty() {
                ctx = ctx.catalog(catalog);
            }
            if !database.is_empty() {
                ctx = ctx.database(database);
            }
            ctx.build()
        })
        .work_group(workgroup)
        .result_configuration(
            aws_sdk_athena::types::ResultConfiguration::builder()
                .output_location(output_location)
                .build(),
        )
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start Athena query: {:?}", e))?;

    result
        .query_execution_id()
        .map(|id| id.to_string())
        .ok_or_else(|| anyhow::anyhow!("No query execution ID returned"))
}

/// Poll query execution status until terminal state.
///
/// Returns the final [`QueryExecution`](aws_sdk_athena::types::QueryExecution).
pub async fn wait_for_query(
    client: &AthenaClient,
    query_id: &str,
    timeout: std::time::Duration,
) -> anyhow::Result<aws_sdk_athena::types::QueryExecution> {
    let start = std::time::Instant::now();
    let poll_interval = std::time::Duration::from_millis(500);

    loop {
        if start.elapsed() > timeout {
            anyhow::bail!(
                "Query {} timed out after {:.0}s",
                query_id,
                timeout.as_secs_f64()
            );
        }

        let response = client
            .get_query_execution()
            .query_execution_id(query_id)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get query status: {:?}", e))?;

        let execution = response
            .query_execution()
            .ok_or_else(|| anyhow::anyhow!("No query execution returned"))?;

        let status = execution
            .status()
            .and_then(|s| s.state())
            .map(|s| s.as_str().to_string())
            .unwrap_or_default();

        match status.as_str() {
            "SUCCEEDED" => return Ok(execution.clone()),
            "FAILED" => {
                let reason = execution
                    .status()
                    .and_then(|s| s.state_change_reason())
                    .unwrap_or("Unknown error");
                anyhow::bail!("Query failed: {}", reason);
            }
            "CANCELLED" => {
                anyhow::bail!("Query was cancelled");
            }
            _ => {
                // QUEUED or RUNNING -- keep polling
                tokio::time::sleep(poll_interval).await;
            }
        }
    }
}

/// Get query results as rows of strings.
///
/// Handles pagination via `NextToken`. Returns `(column_names, rows)` where
/// each row is a `Vec<String>` of cell values.
pub async fn get_query_results(
    client: &AthenaClient,
    query_id: &str,
) -> anyhow::Result<(Vec<String>, Vec<Vec<String>>)> {
    let mut all_rows: Vec<Vec<String>> = Vec::new();
    let mut columns: Vec<String> = Vec::new();
    let mut next_token: Option<String> = None;
    let mut is_first_page = true;

    loop {
        let mut request = client
            .get_query_results()
            .query_execution_id(query_id)
            .max_results(1000);

        if let Some(ref token) = next_token {
            request = request.next_token(token);
        }

        let response = request
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get query results: {:?}", e))?;

        if let Some(result_set) = response.result_set() {
            // Extract column names from metadata (first page only).
            if columns.is_empty() {
                if let Some(metadata) = result_set.result_set_metadata() {
                    columns = metadata
                        .column_info()
                        .iter()
                        .map(|c| c.name().to_string())
                        .collect();
                }
            }

            // Extract rows.
            for (i, row) in result_set.rows().iter().enumerate() {
                // Skip header row on first page.
                if is_first_page && i == 0 {
                    continue;
                }
                let row_data: Vec<String> = row
                    .data()
                    .iter()
                    .map(|d| d.var_char_value().unwrap_or("").to_string())
                    .collect();
                all_rows.push(row_data);
            }
        }

        is_first_page = false;
        next_token = response.next_token().map(|t| t.to_string());
        if next_token.is_none() {
            break;
        }
    }

    Ok((columns, all_rows))
}

/// Convenience: execute a query, wait for completion, return results.
///
/// Uses a default timeout of 120 seconds.
#[allow(dead_code)] // lighter-weight alternative to execute_and_wait_with_stats
pub async fn execute_and_wait(
    client: &AthenaClient,
    sql: &str,
    catalog: &str,
    database: &str,
    workgroup: &str,
    output_location: &str,
) -> anyhow::Result<(Vec<String>, Vec<Vec<String>>)> {
    let query_id = start_query(client, sql, catalog, database, workgroup, output_location).await?;
    let timeout = std::time::Duration::from_secs(120);
    wait_for_query(client, &query_id, timeout).await?;
    get_query_results(client, &query_id).await
}

/// Like [`execute_and_wait`], but also returns the query execution ID and statistics.
///
/// Used by the query audit log to capture per-query metadata.
pub async fn execute_and_wait_with_stats(
    client: &AthenaClient,
    sql: &str,
    catalog: &str,
    database: &str,
    workgroup: &str,
    output_location: &str,
) -> anyhow::Result<QueryExecutionResult> {
    let query_id = start_query(client, sql, catalog, database, workgroup, output_location).await?;
    let timeout = std::time::Duration::from_secs(120);
    let execution = wait_for_query(client, &query_id, timeout).await?;

    let data_scanned = execution
        .statistics()
        .map(|s| s.data_scanned_in_bytes().unwrap_or(0))
        .unwrap_or(0);
    let exec_time = execution
        .statistics()
        .map(|s| s.engine_execution_time_in_millis().unwrap_or(0))
        .unwrap_or(0);

    let (columns, rows) = get_query_results(client, &query_id).await?;

    Ok(QueryExecutionResult {
        query_execution_id: query_id,
        columns,
        rows,
        data_scanned_bytes: data_scanned,
        engine_execution_time_ms: exec_time,
    })
}
