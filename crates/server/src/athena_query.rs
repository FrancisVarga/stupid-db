//! AWS Athena query execution utilities.
//!
//! Provides a reusable Athena client builder from decrypted credentials
//! and helper functions for executing queries with polling.

use aws_sdk_athena::Client as AthenaClient;
use aws_sdk_athena::config::Region;
use aws_credential_types::Credentials;

use crate::athena_connections::{
    AthenaColumn, AthenaConnectionConfig, AthenaConnectionCredentials, AthenaDatabase, AthenaSchema,
    AthenaTable,
};

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
    database: &str,
    workgroup: &str,
    output_location: &str,
) -> anyhow::Result<String> {
    let result = client
        .start_query_execution()
        .query_string(sql)
        .query_execution_context(
            aws_sdk_athena::types::QueryExecutionContext::builder()
                .database(database)
                .build(),
        )
        .work_group(workgroup)
        .result_configuration(
            aws_sdk_athena::types::ResultConfiguration::builder()
                .output_location(output_location)
                .build(),
        )
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to start Athena query: {}", e))?;

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
            .map_err(|e| anyhow::anyhow!("Failed to get query status: {}", e))?;

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
            .map_err(|e| anyhow::anyhow!("Failed to get query results: {}", e))?;

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

/// Fetch the full schema for an Athena connection.
///
/// Discovers databases, tables, and columns using `SHOW DATABASES`,
/// `SHOW TABLES IN`, and `DESCRIBE` SQL queries. Errors on individual
/// databases or tables are logged and skipped rather than failing the
/// entire schema fetch.
pub async fn fetch_schema(
    creds: &AthenaConnectionCredentials,
    conn: &AthenaConnectionConfig,
) -> anyhow::Result<AthenaSchema> {
    let client = build_athena_client(creds).await;
    let workgroup = &conn.workgroup;
    let output_location = &conn.output_location;

    tracing::info!("Fetching schema for Athena connection '{}'...", conn.id);

    // 1. Get all databases.
    let (_, db_rows) = execute_and_wait(
        &client,
        "SHOW DATABASES",
        &conn.database,
        workgroup,
        output_location,
    )
    .await?;

    let mut databases = Vec::new();

    for db_row in &db_rows {
        let db_name = match db_row.first() {
            Some(name) if !name.is_empty() => name.clone(),
            _ => continue,
        };

        tracing::info!("  Discovering tables in database '{}'", db_name);

        // 2. Get tables for this database.
        let tables_sql = format!("SHOW TABLES IN \"{}\"", db_name);
        let tables = match execute_and_wait(
            &client,
            &tables_sql,
            &db_name,
            workgroup,
            output_location,
        )
        .await
        {
            Ok((_, rows)) => {
                let mut table_list = Vec::new();
                for table_row in &rows {
                    let table_name = match table_row.first() {
                        Some(name) if !name.is_empty() => name.clone(),
                        _ => continue,
                    };

                    // 3. Get columns for this table.
                    let describe_sql =
                        format!("DESCRIBE \"{}\".\"{}\"", db_name, table_name);
                    let columns = match execute_and_wait(
                        &client,
                        &describe_sql,
                        &db_name,
                        workgroup,
                        output_location,
                    )
                    .await
                    {
                        Ok((_, col_rows)) => col_rows
                            .iter()
                            .filter_map(|row| {
                                let col_name = row.first()?.clone();
                                if col_name.is_empty() || col_name.starts_with('#') {
                                    return None; // Skip partition info or empty rows
                                }
                                let data_type =
                                    row.get(1).cloned().unwrap_or_default();
                                let comment = row.get(2).and_then(|c| {
                                    if c.is_empty() {
                                        None
                                    } else {
                                        Some(c.clone())
                                    }
                                });
                                Some(AthenaColumn {
                                    name: col_name,
                                    data_type,
                                    comment,
                                })
                            })
                            .collect(),
                        Err(e) => {
                            tracing::warn!(
                                "    Failed to describe {}.{}: {}",
                                db_name,
                                table_name,
                                e
                            );
                            Vec::new()
                        }
                    };

                    table_list.push(AthenaTable {
                        name: table_name,
                        columns,
                    });
                }
                table_list
            }
            Err(e) => {
                tracing::warn!("  Failed to list tables in '{}': {}", db_name, e);
                Vec::new()
            }
        };

        databases.push(AthenaDatabase {
            name: db_name,
            tables,
        });
    }

    let schema = AthenaSchema {
        databases,
        fetched_at: chrono::Utc::now().to_rfc3339(),
    };

    tracing::info!(
        "Schema fetch complete: {} databases, {} total tables",
        schema.databases.len(),
        schema
            .databases
            .iter()
            .map(|d| d.tables.len())
            .sum::<usize>()
    );

    Ok(schema)
}

/// Convenience: execute a query, wait for completion, return results.
///
/// Uses a default timeout of 120 seconds.
pub async fn execute_and_wait(
    client: &AthenaClient,
    sql: &str,
    database: &str,
    workgroup: &str,
    output_location: &str,
) -> anyhow::Result<(Vec<String>, Vec<Vec<String>>)> {
    let query_id = start_query(client, sql, database, workgroup, output_location).await?;
    let timeout = std::time::Duration::from_secs(120);
    wait_for_query(client, &query_id, timeout).await?;
    get_query_results(client, &query_id).await
}
