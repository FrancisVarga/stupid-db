//! Athena schema discovery via parallel query execution.
//!
//! Discovers databases, tables, and columns using `SHOW DATABASES`,
//! `SHOW TABLES IN`, and `DESCRIBE` SQL queries with bounded concurrency.

use std::sync::Arc;

use futures::future::join_all;
use tokio::sync::Semaphore;

use crate::athena_connections::{
    AthenaColumn, AthenaConnectionConfig, AthenaConnectionCredentials, AthenaDatabase, AthenaSchema,
    AthenaTable,
};

use super::execution::{build_athena_client, execute_and_wait_with_stats};

/// Default maximum number of concurrent Athena queries during schema discovery.
const DEFAULT_SCHEMA_CONCURRENCY: usize = 10;

/// Fetch the full schema for an Athena connection using parallel discovery.
///
/// Discovers databases, tables, and columns using `SHOW DATABASES`,
/// `SHOW TABLES IN`, and `DESCRIBE` SQL queries. Table listing and column
/// discovery run in parallel (bounded by `DEFAULT_SCHEMA_CONCURRENCY`) to
/// minimize wall-clock time. Errors on individual databases or tables are
/// logged and skipped rather than failing the entire schema fetch.
pub async fn fetch_schema(
    creds: &AthenaConnectionCredentials,
    conn: &AthenaConnectionConfig,
    query_log: Option<&crate::athena_query_log::AthenaQueryLog>,
) -> anyhow::Result<AthenaSchema> {
    let client = Arc::new(build_athena_client(creds).await);
    let semaphore = Arc::new(Semaphore::new(DEFAULT_SCHEMA_CONCURRENCY));
    let catalog = conn.catalog.clone();
    let workgroup = conn.workgroup.clone();
    let output_location = conn.output_location.clone();
    let connection_id = conn.id.clone();

    tracing::info!("Fetching schema for Athena connection '{}'...", conn.id);

    // Phase 0: Get all databases (sequential — must complete first).
    let show_db_sql = "SHOW DATABASES";
    let show_db_start = chrono::Utc::now();
    let show_db_wall = std::time::Instant::now();
    let db_result = execute_and_wait_with_stats(
        &client,
        show_db_sql,
        &catalog,
        &conn.database,
        &workgroup,
        &output_location,
    )
    .await;

    let db_result = match db_result {
        Ok(r) => {
            if let Some(log) = query_log {
                let now = chrono::Utc::now();
                log.append(crate::athena_query_log::AthenaQueryLogEntry {
                    entry_id: 0,
                    connection_id: connection_id.clone(),
                    query_execution_id: Some(r.query_execution_id.clone()),
                    source: crate::athena_query_log::QuerySource::SchemaRefreshDatabases,
                    sql: show_db_sql.into(),
                    database: conn.database.clone(),
                    workgroup: workgroup.clone(),
                    outcome: crate::athena_query_log::QueryOutcome::Succeeded,
                    error_message: None,
                    data_scanned_bytes: r.data_scanned_bytes,
                    engine_execution_time_ms: r.engine_execution_time_ms,
                    total_rows: Some(r.rows.len() as u64),
                    estimated_cost_usd: crate::athena_query_log::calculate_query_cost(r.data_scanned_bytes),
                    started_at: show_db_start,
                    completed_at: now,
                    wall_clock_ms: show_db_wall.elapsed().as_millis() as i64,
                });
            }
            r
        }
        Err(e) => {
            if let Some(log) = query_log {
                let now = chrono::Utc::now();
                log.append(crate::athena_query_log::AthenaQueryLogEntry {
                    entry_id: 0,
                    connection_id: connection_id.clone(),
                    query_execution_id: None,
                    source: crate::athena_query_log::QuerySource::SchemaRefreshDatabases,
                    sql: show_db_sql.into(),
                    database: conn.database.clone(),
                    workgroup: workgroup.clone(),
                    outcome: crate::athena_query_log::QueryOutcome::Failed,
                    error_message: Some(e.to_string()),
                    data_scanned_bytes: 0,
                    engine_execution_time_ms: 0,
                    total_rows: None,
                    estimated_cost_usd: 0.0,
                    started_at: show_db_start,
                    completed_at: now,
                    wall_clock_ms: show_db_wall.elapsed().as_millis() as i64,
                });
            }
            return Err(e);
        }
    };
    let db_rows = db_result.rows;

    let db_names: Vec<String> = db_rows
        .iter()
        .filter_map(|row| {
            row.first()
                .filter(|name| !name.is_empty())
                .cloned()
        })
        .collect();

    tracing::info!(
        "Found {} databases, discovering tables in parallel...",
        db_names.len()
    );

    // Phase 1: SHOW TABLES IN {db} — parallel across all databases.
    let table_futures = db_names.iter().map(|db_name| {
        let client = Arc::clone(&client);
        let sem = Arc::clone(&semaphore);
        let catalog = catalog.clone();
        let workgroup = workgroup.clone();
        let output_location = output_location.clone();
        let db_name = db_name.clone();
        let connection_id = connection_id.clone();

        async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let tables_sql = format!("SHOW TABLES IN `{}`", db_name);
            let started_at = chrono::Utc::now();
            let wall = std::time::Instant::now();
            let result = execute_and_wait_with_stats(
                &client,
                &tables_sql,
                &catalog,
                &db_name,
                &workgroup,
                &output_location,
            )
            .await;

            match result {
                Ok(r) => {
                    if let Some(log) = query_log {
                        let now = chrono::Utc::now();
                        log.append(crate::athena_query_log::AthenaQueryLogEntry {
                            entry_id: 0,
                            connection_id: connection_id.clone(),
                            query_execution_id: Some(r.query_execution_id.clone()),
                            source: crate::athena_query_log::QuerySource::SchemaRefreshTables,
                            sql: tables_sql,
                            database: db_name.clone(),
                            workgroup: workgroup.clone(),
                            outcome: crate::athena_query_log::QueryOutcome::Succeeded,
                            error_message: None,
                            data_scanned_bytes: r.data_scanned_bytes,
                            engine_execution_time_ms: r.engine_execution_time_ms,
                            total_rows: Some(r.rows.len() as u64),
                            estimated_cost_usd: crate::athena_query_log::calculate_query_cost(r.data_scanned_bytes),
                            started_at,
                            completed_at: now,
                            wall_clock_ms: wall.elapsed().as_millis() as i64,
                        });
                    }
                    let table_names: Vec<String> = r.rows
                        .iter()
                        .filter_map(|row| {
                            row.first()
                                .filter(|name| !name.is_empty())
                                .cloned()
                        })
                        .collect();
                    tracing::info!(
                        "  Database '{}': {} tables",
                        db_name,
                        table_names.len()
                    );
                    (db_name, table_names)
                }
                Err(e) => {
                    if let Some(log) = query_log {
                        let now = chrono::Utc::now();
                        log.append(crate::athena_query_log::AthenaQueryLogEntry {
                            entry_id: 0,
                            connection_id: connection_id.clone(),
                            query_execution_id: None,
                            source: crate::athena_query_log::QuerySource::SchemaRefreshTables,
                            sql: tables_sql,
                            database: db_name.clone(),
                            workgroup: workgroup.clone(),
                            outcome: crate::athena_query_log::QueryOutcome::Failed,
                            error_message: Some(e.to_string()),
                            data_scanned_bytes: 0,
                            engine_execution_time_ms: 0,
                            total_rows: None,
                            estimated_cost_usd: 0.0,
                            started_at,
                            completed_at: now,
                            wall_clock_ms: wall.elapsed().as_millis() as i64,
                        });
                    }
                    tracing::warn!("  Failed to list tables in '{}': {}", db_name, e);
                    (db_name, Vec::new())
                }
            }
        }
    });

    let db_tables: Vec<(String, Vec<String>)> = join_all(table_futures).await;

    // Flatten into (db_name, table_name) pairs for phase 2.
    let all_pairs: Vec<(String, String)> = db_tables
        .iter()
        .flat_map(|(db, tables)| {
            tables.iter().map(move |t| (db.clone(), t.clone()))
        })
        .collect();

    tracing::info!(
        "Discovering columns for {} tables in parallel...",
        all_pairs.len()
    );

    // Phase 2: DESCRIBE {db}.{table} — parallel across all tables.
    let column_futures = all_pairs.iter().map(|(db_name, table_name)| {
        let client = Arc::clone(&client);
        let sem = Arc::clone(&semaphore);
        let catalog = catalog.clone();
        let workgroup = workgroup.clone();
        let output_location = output_location.clone();
        let db_name = db_name.clone();
        let table_name = table_name.clone();
        let connection_id = connection_id.clone();

        async move {
            let _permit = sem.acquire().await.expect("semaphore closed");
            let describe_sql = format!("DESCRIBE `{}`.`{}`", db_name, table_name);
            let started_at = chrono::Utc::now();
            let wall = std::time::Instant::now();
            let columns = match execute_and_wait_with_stats(
                &client,
                &describe_sql,
                &catalog,
                &db_name,
                &workgroup,
                &output_location,
            )
            .await
            {
                Ok(r) => {
                    if let Some(log) = query_log {
                        let now = chrono::Utc::now();
                        log.append(crate::athena_query_log::AthenaQueryLogEntry {
                            entry_id: 0,
                            connection_id: connection_id.clone(),
                            query_execution_id: Some(r.query_execution_id.clone()),
                            source: crate::athena_query_log::QuerySource::SchemaRefreshDescribe,
                            sql: describe_sql.clone(),
                            database: db_name.clone(),
                            workgroup: workgroup.clone(),
                            outcome: crate::athena_query_log::QueryOutcome::Succeeded,
                            error_message: None,
                            data_scanned_bytes: r.data_scanned_bytes,
                            engine_execution_time_ms: r.engine_execution_time_ms,
                            total_rows: Some(r.rows.len() as u64),
                            estimated_cost_usd: crate::athena_query_log::calculate_query_cost(r.data_scanned_bytes),
                            started_at,
                            completed_at: now,
                            wall_clock_ms: wall.elapsed().as_millis() as i64,
                        });
                    }
                    r.rows
                        .iter()
                        .filter_map(|row| {
                            let raw = row.first()?.clone();
                            if raw.is_empty() || raw.starts_with('#') {
                                return None;
                            }

                            // Athena DESCRIBE returns a single cell per row with
                            // tab-delimited "col_name\tdata_type\tcomment".
                            // Fall back to multi-column access if no tabs found.
                            let (col_name, data_type, comment) = if raw.contains('\t') {
                                let mut parts = raw.splitn(3, '\t');
                                let name = parts.next().unwrap_or("").trim().to_string();
                                let dtype = parts.next().unwrap_or("").trim().to_string();
                                let cmt = parts.next()
                                    .map(|s| s.trim().to_string())
                                    .filter(|s| !s.is_empty());
                                (name, dtype, cmt)
                            } else {
                                let name = raw.trim().to_string();
                                let dtype = row.get(1).map(|s| s.trim().to_string()).unwrap_or_default();
                                let cmt = row.get(2).and_then(|c| {
                                    let t = c.trim();
                                    if t.is_empty() { None } else { Some(t.to_string()) }
                                });
                                (name, dtype, cmt)
                            };

                            if col_name.is_empty() {
                                return None;
                            }

                            Some(AthenaColumn {
                                name: col_name,
                                data_type,
                                comment,
                            })
                        })
                        .collect()
                }
                Err(e) => {
                    if let Some(log) = query_log {
                        let now = chrono::Utc::now();
                        log.append(crate::athena_query_log::AthenaQueryLogEntry {
                            entry_id: 0,
                            connection_id: connection_id.clone(),
                            query_execution_id: None,
                            source: crate::athena_query_log::QuerySource::SchemaRefreshDescribe,
                            sql: describe_sql,
                            database: db_name.clone(),
                            workgroup: workgroup.clone(),
                            outcome: crate::athena_query_log::QueryOutcome::Failed,
                            error_message: Some(e.to_string()),
                            data_scanned_bytes: 0,
                            engine_execution_time_ms: 0,
                            total_rows: None,
                            estimated_cost_usd: 0.0,
                            started_at,
                            completed_at: now,
                            wall_clock_ms: wall.elapsed().as_millis() as i64,
                        });
                    }
                    tracing::warn!(
                        "    Failed to describe {}.{}: {}",
                        db_name,
                        table_name,
                        e
                    );
                    Vec::new()
                }
            };

            (db_name, table_name, columns)
        }
    });

    let column_results: Vec<(String, String, Vec<AthenaColumn>)> =
        join_all(column_futures).await;

    // Phase 3: Reassemble into the database → tables hierarchy.
    // Preserve the original database order from SHOW DATABASES.
    let mut db_table_map: std::collections::HashMap<String, Vec<AthenaTable>> =
        std::collections::HashMap::new();

    for (db_name, table_name, columns) in column_results {
        db_table_map
            .entry(db_name)
            .or_default()
            .push(AthenaTable {
                name: table_name,
                columns,
            });
    }

    let databases: Vec<AthenaDatabase> = db_names
        .into_iter()
        .map(|db_name| {
            let tables = db_table_map.remove(&db_name).unwrap_or_default();
            AthenaDatabase {
                name: db_name,
                tables,
            }
        })
        .collect();

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
