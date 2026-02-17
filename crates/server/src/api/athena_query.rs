//! Athena SQL execution endpoints: SSE streaming query, Parquet download,
//! schema introspection, and query audit log.
//!
//! SRP: Athena SQL execution and schema management.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use serde::Deserialize;
use tokio_stream::wrappers::ReceiverStream;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

use super::QueryErrorResponse;

// ── Request types ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct AthenaQueryRequest {
    pub sql: String,
    #[serde(default)]
    pub database: Option<String>,
}

// ── SSE streaming query ──────────────────────────────────────────

/// SSE streaming Athena query execution.
///
/// Submits a SQL query to AWS Athena via the specified connection, polls for
/// status updates, and streams results back as Server-Sent Events.
///
/// Events emitted:
/// - `status`  — query state transitions (QUEUED, RUNNING, SUCCEEDED) with stats
/// - `columns` — column metadata (name + type) sent once before row data
/// - `rows`    — batches of up to 100 result rows
/// - `done`    — final summary (total_rows, data_scanned_bytes, execution_time_ms)
/// - `error`   — terminal error with message
pub async fn athena_query_sse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AthenaQueryRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // 1. Get credentials and connection config.
    let store = state.athena_connections.read().await;
    let creds = match store.get_credentials(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: "Connection not found".into(),
                }),
            ))
        }
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            ))
        }
    };
    let conn = match store.get(&id) {
        Ok(Some(c)) => c,
        _ => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: "Connection not found".into(),
                }),
            ))
        }
    };
    drop(store);

    let catalog = conn.catalog.clone();
    let database = req.database.unwrap_or_else(|| conn.database.clone());
    let workgroup = conn.workgroup.clone();
    let output_location = conn.output_location.clone();
    let sql = req.sql.clone();
    let connection_id_for_log = id.clone();
    let state_for_log = state.clone();

    // 2. Create a channel-based stream.
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    // 3. Spawn background task to execute query and stream events.
    tokio::spawn(async move {
        let client = crate::athena_query::build_athena_client(&creds).await;
        let wall_start = std::time::Instant::now();
        let log_sql = sql.clone();
        let log_db = database.clone();
        let log_wg = workgroup.clone();
        let log_conn_id = connection_id_for_log;

        // Helper: append a query log entry at each terminal state.
        macro_rules! log_query {
            ($outcome:expr, $qid:expr, $scanned:expr, $exec_ms:expr, $rows:expr, $err:expr) => {
                let now = chrono::Utc::now();
                state_for_log.athena_query_log.append(
                    crate::athena_query_log::AthenaQueryLogEntry {
                        entry_id: 0,
                        connection_id: log_conn_id.clone(),
                        query_execution_id: $qid,
                        source: crate::athena_query_log::QuerySource::UserQuery,
                        sql: log_sql.clone(),
                        database: log_db.clone(),
                        workgroup: log_wg.clone(),
                        outcome: $outcome,
                        error_message: $err,
                        data_scanned_bytes: $scanned,
                        engine_execution_time_ms: $exec_ms,
                        total_rows: $rows,
                        estimated_cost_usd: crate::athena_query_log::calculate_query_cost($scanned),
                        started_at: now,
                        completed_at: now,
                        wall_clock_ms: wall_start.elapsed().as_millis() as i64,
                    },
                );
            };
        }

        // Start query.
        let query_id = match crate::athena_query::start_query(
            &client,
            &sql,
            &catalog,
            &database,
            &workgroup,
            &output_location,
        )
        .await
        {
            Ok(id) => {
                let _ = tx
                    .send(Ok(Event::default().event("status").data(
                        serde_json::json!({"state": "QUEUED", "query_id": &id}).to_string(),
                    )))
                    .await;
                id
            }
            Err(e) => {
                log_query!(
                    crate::athena_query_log::QueryOutcome::Failed,
                    None, 0, 0, None,
                    Some(e.to_string())
                );
                let _ = tx
                    .send(Ok(Event::default().event("error").data(
                        serde_json::json!({"message": e.to_string()}).to_string(),
                    )))
                    .await;
                return;
            }
        };

        // Poll for status updates.
        let timeout = std::time::Duration::from_secs(120);
        let start = std::time::Instant::now();
        let poll_interval = std::time::Duration::from_millis(500);

        loop {
            if start.elapsed() > timeout {
                log_query!(
                    crate::athena_query_log::QueryOutcome::TimedOut,
                    Some(query_id.clone()), 0, 0, None,
                    Some("Query timed out after 120s".into())
                );
                let _ = tx
                    .send(Ok(Event::default().event("error").data(
                        serde_json::json!({"message": "Query timed out after 120s"}).to_string(),
                    )))
                    .await;
                return;
            }

            let response = match client
                .get_query_execution()
                .query_execution_id(&query_id)
                .send()
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": e.to_string()}).to_string(),
                        )))
                        .await;
                    return;
                }
            };

            let execution = match response.query_execution() {
                Some(e) => e,
                None => {
                    tokio::time::sleep(poll_interval).await;
                    continue;
                }
            };

            let state_str = execution
                .status()
                .and_then(|s| s.state())
                .map(|s| s.as_str().to_string())
                .unwrap_or_default();

            let data_scanned = execution
                .statistics()
                .map(|s| s.data_scanned_in_bytes().unwrap_or(0))
                .unwrap_or(0);

            match state_str.as_str() {
                "SUCCEEDED" => {
                    let exec_time_ms = execution
                        .statistics()
                        .map(|s| s.engine_execution_time_in_millis().unwrap_or(0))
                        .unwrap_or(0);

                    let _ = tx
                        .send(Ok(Event::default().event("status").data(
                            serde_json::json!({
                                "state": "SUCCEEDED",
                                "data_scanned_bytes": data_scanned,
                                "execution_time_ms": exec_time_ms
                            })
                            .to_string(),
                        )))
                        .await;

                    // Stream results in batches of 100.
                    let mut next_token: Option<String> = None;
                    let mut is_first_page = true;
                    let mut total_rows = 0u64;

                    loop {
                        let mut request = client
                            .get_query_results()
                            .query_execution_id(&query_id)
                            .max_results(100);

                        if let Some(ref token) = next_token {
                            request = request.next_token(token);
                        }

                        match request.send().await {
                            Ok(result_response) => {
                                if let Some(result_set) = result_response.result_set() {
                                    // Send column metadata on first page only.
                                    if is_first_page {
                                        if let Some(metadata) = result_set.result_set_metadata() {
                                            let columns: Vec<serde_json::Value> = metadata
                                                .column_info()
                                                .iter()
                                                .map(|c| {
                                                    serde_json::json!({
                                                        "name": c.name(),
                                                        "type": c.r#type()
                                                    })
                                                })
                                                .collect();
                                            let _ = tx
                                                .send(Ok(Event::default().event("columns").data(
                                                    serde_json::json!({"columns": columns})
                                                        .to_string(),
                                                )))
                                                .await;
                                        }
                                    }

                                    // Send rows (skip header row on first page).
                                    let mut batch_rows: Vec<Vec<String>> = Vec::new();
                                    for (i, row) in result_set.rows().iter().enumerate() {
                                        if is_first_page && i == 0 {
                                            continue;
                                        }
                                        let row_data: Vec<String> = row
                                            .data()
                                            .iter()
                                            .map(|d| {
                                                d.var_char_value().unwrap_or("").to_string()
                                            })
                                            .collect();
                                        batch_rows.push(row_data);
                                    }

                                    if !batch_rows.is_empty() {
                                        total_rows += batch_rows.len() as u64;
                                        let _ = tx
                                            .send(Ok(Event::default().event("rows").data(
                                                serde_json::json!({"rows": batch_rows})
                                                    .to_string(),
                                            )))
                                            .await;
                                    }
                                }

                                is_first_page = false;
                                next_token =
                                    result_response.next_token().map(|t| t.to_string());
                                if next_token.is_none() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx
                                    .send(Ok(Event::default().event("error").data(
                                        serde_json::json!({
                                            "message": format!("Failed to get results: {}", e)
                                        })
                                        .to_string(),
                                    )))
                                    .await;
                                return;
                            }
                        }
                    }

                    // Send done event.
                    let _ = tx
                        .send(Ok(Event::default().event("done").data(
                            serde_json::json!({
                                "total_rows": total_rows,
                                "data_scanned_bytes": data_scanned,
                                "execution_time_ms": exec_time_ms
                            })
                            .to_string(),
                        )))
                        .await;
                    log_query!(
                        crate::athena_query_log::QueryOutcome::Succeeded,
                        Some(query_id.clone()), data_scanned, exec_time_ms,
                        Some(total_rows), None
                    );
                    return;
                }
                "FAILED" => {
                    let reason = execution
                        .status()
                        .and_then(|s| s.state_change_reason())
                        .unwrap_or("Unknown error");
                    log_query!(
                        crate::athena_query_log::QueryOutcome::Failed,
                        Some(query_id.clone()), data_scanned, 0, None,
                        Some(reason.to_string())
                    );
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": reason}).to_string(),
                        )))
                        .await;
                    return;
                }
                "CANCELLED" => {
                    log_query!(
                        crate::athena_query_log::QueryOutcome::Cancelled,
                        Some(query_id.clone()), data_scanned, 0, None, None
                    );
                    let _ = tx
                        .send(Ok(Event::default().event("error").data(
                            serde_json::json!({"message": "Query was cancelled"}).to_string(),
                        )))
                        .await;
                    return;
                }
                _ => {
                    // QUEUED or RUNNING — send status update and keep polling.
                    let _ = tx
                        .send(Ok(Event::default().event("status").data(
                            serde_json::json!({
                                "state": state_str,
                                "data_scanned_bytes": data_scanned
                            })
                            .to_string(),
                        )))
                        .await;
                    tokio::time::sleep(poll_interval).await;
                }
            }
        }
    });

    // Convert mpsc receiver to a stream.
    let stream = ReceiverStream::new(rx);
    Ok(Sse::new(stream))
}

// ── Parquet download ─────────────────────────────────────────────

/// Execute an Athena query and return the results as a Parquet file download.
///
/// Uses the same query execution flow as the SSE endpoint but collects all
/// results into memory and returns a single Parquet file with proper types
/// and Zstd compression. The response includes Content-Disposition header
/// for browser download.
pub async fn athena_query_parquet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AthenaQueryRequest>,
) -> Result<
    axum::response::Response,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // 1. Get credentials and connection config.
    let store = state.athena_connections.read().await;
    let creds = match store.get_credentials(&id) {
        Ok(Some(c)) => c,
        Ok(None) => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse { error: "Connection not found".into() }),
            ))
        }
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse { error: e.to_string() }),
            ))
        }
    };
    let conn = match store.get(&id) {
        Ok(Some(c)) => c,
        _ => {
            return Err((
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse { error: "Connection not found".into() }),
            ))
        }
    };
    drop(store);

    let catalog = conn.catalog.clone();
    let database = req.database.unwrap_or_else(|| conn.database.clone());
    let workgroup = conn.workgroup.clone();
    let output_location = conn.output_location.clone();

    // 2. Execute query and collect all results.
    let client = crate::athena_query::build_athena_client(&creds).await;
    let result = crate::athena_query::execute_and_wait_with_stats(
        &client,
        &req.sql,
        &catalog,
        &database,
        &workgroup,
        &output_location,
    )
    .await
    .map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse { error: e.to_string() }),
        )
    })?;

    // 3. Convert to AthenaQueryResult for the parquet module.
    let athena_result = stupid_athena::AthenaQueryResult {
        columns: result
            .columns
            .iter()
            .map(|name| stupid_athena::AthenaColumn {
                name: name.clone(),
                data_type: "varchar".into(), // column type info not available from execute_and_wait_with_stats
            })
            .collect(),
        rows: result
            .rows
            .into_iter()
            .map(|row| row.into_iter().map(|cell| {
                if cell.is_empty() { None } else { Some(cell) }
            }).collect())
            .collect(),
        metadata: stupid_athena::QueryMetadata {
            query_id: result.query_execution_id.clone(),
            bytes_scanned: result.data_scanned_bytes as u64,
            execution_time_ms: result.engine_execution_time_ms as u64,
            state: "SUCCEEDED".into(),
            output_location: None,
        },
    };

    // 4. Write Parquet to in-memory buffer.
    let parquet_bytes = stupid_athena::write_parquet_bytes(&athena_result).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse { error: format!("Parquet write error: {}", e) }),
        )
    })?;

    // 5. Also persist to data/exports/ for later reference.
    let exports_dir = state.data_dir.join("exports").join("athena");
    let filename = format!("{}.parquet", result.query_execution_id);
    let export_path = exports_dir.join(&filename);
    if let Err(e) = std::fs::create_dir_all(&exports_dir) {
        tracing::warn!("Failed to create exports dir: {}", e);
    } else if let Err(e) = std::fs::write(&export_path, &parquet_bytes) {
        tracing::warn!("Failed to persist parquet export: {}", e);
    } else {
        tracing::info!(
            path = %export_path.display(),
            rows = athena_result.rows.len(),
            bytes = parquet_bytes.len(),
            "Persisted Parquet export"
        );
    }

    // 6. Return as downloadable file.
    Ok(axum::response::Response::builder()
        .status(200)
        .header("Content-Type", "application/vnd.apache.parquet")
        .header(
            "Content-Disposition",
            format!("attachment; filename=\"{}\"", filename),
        )
        .header("Content-Length", parquet_bytes.len().to_string())
        .body(axum::body::Body::from(parquet_bytes))
        .unwrap())
}

// ── Schema endpoints ─────────────────────────────────────────────

/// Get cached schema for an Athena connection.
pub async fn athena_connections_schema(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, axum::http::StatusCode> {
    let store = state.athena_connections.read().await;
    match store.get(&id) {
        Ok(Some(conn)) => {
            Ok(Json(serde_json::json!({
                "schema_status": conn.schema_status,
                "schema": conn.schema,
            })))
        }
        Ok(None) => Err(axum::http::StatusCode::NOT_FOUND),
        Err(_) => Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
    }
}

/// Trigger background schema refresh for an Athena connection.
pub async fn athena_connections_schema_refresh(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // Get credentials and connection config.
    let (creds, conn) = {
        let store = state.athena_connections.read().await;
        let creds = match store.get_credentials(&id) {
            Ok(Some(c)) => c,
            Ok(None) => return Err((axum::http::StatusCode::NOT_FOUND, Json(QueryErrorResponse { error: "Not found".into() }))),
            Err(e) => return Err((axum::http::StatusCode::INTERNAL_SERVER_ERROR, Json(QueryErrorResponse { error: e.to_string() }))),
        };
        let conn = match store.get(&id) {
            Ok(Some(c)) => c,
            _ => return Err((axum::http::StatusCode::NOT_FOUND, Json(QueryErrorResponse { error: "Not found".into() }))),
        };
        (creds, conn)
    };

    // Update status to "fetching".
    {
        let store = state.athena_connections.read().await;
        let _ = store.update_schema_status(&id, "fetching");
    }

    // Spawn background schema fetch.
    let state_clone = state.clone();
    let id_clone = id.clone();
    tokio::spawn(async move {
        match crate::athena_query::fetch_schema(&creds, &conn, Some(&state_clone.athena_query_log)).await {
            Ok(schema) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema(&id_clone, schema);
                tracing::info!("Schema refresh complete for Athena connection '{}'", id_clone);
                drop(store);

                // Rebuild catalog external sources from all Athena connections.
                rebuild_catalog_external_sources(&state_clone).await;
            }
            Err(e) => {
                let store = state_clone.athena_connections.read().await;
                let _ = store.update_schema_status(&id_clone, &format!("failed: {}", e));
                tracing::warn!("Schema refresh failed for '{}': {}", id_clone, e);
            }
        }
    });

    Ok(Json(serde_json::json!({ "status": "fetching", "message": "Schema refresh started" })))
}

/// Rebuild the catalog's external SQL sources from all enabled Athena connections.
///
/// Reads all Athena connections with cached schemas, converts them to
/// `ExternalSource` entries, merges into the in-memory catalog, and
/// persists `current.json` to the catalog store.
async fn rebuild_catalog_external_sources(state: &Arc<AppState>) {
    let athena_store = state.athena_connections.read().await;
    let conns = match athena_store.list() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to list Athena connections for catalog update: {}", e);
            return;
        }
    };

    let sources: Vec<stupid_catalog::ExternalSource> = conns
        .iter()
        .filter(|c| c.enabled && c.schema.is_some())
        .map(|c| {
            let schema = c.schema.as_ref().unwrap();
            stupid_catalog::ExternalSource {
                name: c.name.clone(),
                kind: "athena".to_string(),
                connection_id: c.id.clone(),
                databases: schema
                    .databases
                    .iter()
                    .map(|db| stupid_catalog::ExternalDatabase {
                        name: db.name.clone(),
                        tables: db
                            .tables
                            .iter()
                            .map(|t| stupid_catalog::ExternalTable {
                                name: t.name.clone(),
                                columns: t
                                    .columns
                                    .iter()
                                    .map(|col| stupid_catalog::ExternalColumn {
                                        name: col.name.clone(),
                                        data_type: col.data_type.clone(),
                                    })
                                    .collect(),
                            })
                            .collect(),
                    })
                    .collect(),
            }
        })
        .collect();
    drop(athena_store);

    // Persist each external source to catalog/external/{kind}-{id}.json
    for source in &sources {
        if let Err(e) = state.catalog_store.save_external_source(source) {
            tracing::warn!("Failed to persist external source '{}': {}", source.connection_id, e);
        }
    }

    // Update the in-memory catalog with refreshed external sources.
    let mut catalog_lock = state.catalog.write().await;
    if let Some(ref mut cat) = *catalog_lock {
        cat.external_sources = sources;
        tracing::info!(
            "Catalog updated with {} external source(s) and persisted to catalog/external/",
            cat.external_sources.len()
        );
    } else {
        tracing::debug!("Catalog not yet built — skipping external source update");
    }
}

// ── Query log ────────────────────────────────────────────────────

/// Get query audit log for an Athena connection.
///
/// Returns matching log entries (newest first) with cumulative and daily cost
/// summaries. Supports filtering by source, outcome, time range, SQL text,
/// and result limit.
pub async fn athena_connections_query_log(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Query(params): Query<crate::athena_query_log::QueryLogParams>,
) -> Result<Json<crate::athena_query_log::QueryLogResponse>, axum::http::StatusCode> {
    // Verify connection exists.
    {
        let store = state.athena_connections.read().await;
        match store.get(&id) {
            Ok(Some(_)) => {}
            Ok(None) => return Err(axum::http::StatusCode::NOT_FOUND),
            Err(_) => return Err(axum::http::StatusCode::INTERNAL_SERVER_ERROR),
        }
    }

    let entries = state.athena_query_log.query(&id, &params);
    let summary = state.athena_query_log.summary(&id);

    Ok(Json(crate::athena_query_log::QueryLogResponse {
        connection_id: id,
        entries,
        summary,
    }))
}
