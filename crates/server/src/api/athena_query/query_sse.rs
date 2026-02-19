//! SSE streaming Athena query execution endpoint.

use std::convert::Infallible;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use tokio_stream::wrappers::ReceiverStream;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

use super::helpers::{athena_response_to_sse, eb_athena_error};
use super::types::AthenaQueryRequest;
use crate::api::QueryErrorResponse;

/// SSE streaming Athena query execution
///
/// Submits a SQL query to AWS Athena via the specified connection, polls for
/// status updates, and streams results back as Server-Sent Events.
///
/// Events emitted:
/// - `status`  -- query state transitions (QUEUED, RUNNING, SUCCEEDED) with stats
/// - `columns` -- column metadata (name + type) sent once before row data
/// - `rows`    -- batches of up to 100 result rows
/// - `done`    -- final summary (total_rows, data_scanned_bytes, execution_time_ms)
/// - `error`   -- terminal error with message
#[utoipa::path(
    post,
    path = "/athena-connections/{id}/query",
    tag = "Athena Queries",
    request_body = AthenaQueryRequest,
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "SSE event stream", content_type = "text/event-stream"),
        (status = 404, description = "Connection not found", body = QueryErrorResponse)
    )
)]
pub async fn athena_query_sse(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AthenaQueryRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // Route through eisenbahn if available AND the athena service is configured.
    if let Some(ref eb) = state.eisenbahn {
        if eb.has_service("athena") {
            let svc_req = stupid_eisenbahn::services::AthenaServiceRequest::Query {
                connection_id: id.clone(),
                sql: req.sql.clone(),
                database: req.database.clone(),
            };
            let mut zmq_rx = eb
                .athena_query_stream(svc_req)
                .await
                .map_err(|e| eb_athena_error(e))?;

            // Bridge the ZMQ stream to SSE events via a channel.
            let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);
            let state_for_log = state.clone();
            let log_conn_id = id.clone();
            let log_sql = req.sql.clone();
            let log_db = req.database.clone().unwrap_or_default();
            let wall_start = std::time::Instant::now();
            tokio::spawn(async move {
                let mut total_rows: Option<u64> = None;
                let mut data_scanned: i64 = 0;
                let mut exec_time_ms: i64 = 0;
                let mut outcome = crate::athena_query_log::QueryOutcome::Failed;
                let mut error_message: Option<String> = None;
                let query_execution_id: Option<String> = None;

                while let Some(result) = zmq_rx.recv().await {
                    let event = match result {
                        Ok(msg) => {
                            // Decode AthenaServiceResponse and map to SSE event types.
                            match msg.decode::<stupid_eisenbahn::services::AthenaServiceResponse>() {
                                Ok(resp) => {
                                    // Track stats for query log.
                                    match &resp {
                                        stupid_eisenbahn::services::AthenaServiceResponse::Done { total_rows: tr } => {
                                            total_rows = *tr;
                                            outcome = crate::athena_query_log::QueryOutcome::Succeeded;
                                        }
                                        stupid_eisenbahn::services::AthenaServiceResponse::Error { message } => {
                                            error_message = Some(message.clone());
                                            outcome = crate::athena_query_log::QueryOutcome::Failed;
                                        }
                                        stupid_eisenbahn::services::AthenaServiceResponse::Status { state, stats } => {
                                            if let Some(stats) = stats {
                                                if let Some(scanned) = stats.get("data_scanned_bytes").and_then(|v| v.as_i64()) {
                                                    data_scanned = scanned;
                                                }
                                                if let Some(exec) = stats.get("execution_time_ms").and_then(|v| v.as_i64()) {
                                                    exec_time_ms = exec;
                                                }
                                            }
                                            if state == "SUCCEEDED" {
                                                outcome = crate::athena_query_log::QueryOutcome::Succeeded;
                                            }
                                        }
                                        _ => {}
                                    }
                                    athena_response_to_sse(resp)
                                }
                                Err(e) => {
                                    tracing::warn!(error = %e, "failed to decode athena service response");
                                    error_message = Some(format!("failed to decode service response: {}", e));
                                    Event::default()
                                        .event("error")
                                        .data(serde_json::json!({"message": format!("failed to decode service response: {}", e)}).to_string())
                                }
                            }
                        }
                        Err(e) => {
                            error_message = Some(e.to_string());
                            Event::default()
                                .event("error")
                                .data(serde_json::json!({"message": e.to_string()}).to_string())
                        }
                    };
                    if tx.send(Ok(event)).await.is_err() {
                        break;
                    }
                }

                // Log query to audit log.
                let now = chrono::Utc::now();
                state_for_log.athena_query_log.append(crate::athena_query_log::AthenaQueryLogEntry {
                    entry_id: 0,
                    connection_id: log_conn_id,
                    query_execution_id,
                    source: crate::athena_query_log::QuerySource::UserQuery,
                    sql: log_sql,
                    database: log_db,
                    workgroup: String::new(),
                    outcome,
                    error_message,
                    data_scanned_bytes: data_scanned,
                    engine_execution_time_ms: exec_time_ms,
                    total_rows,
                    estimated_cost_usd: crate::athena_query_log::calculate_query_cost(data_scanned),
                    started_at: now,
                    completed_at: now,
                    wall_clock_ms: wall_start.elapsed().as_millis() as i64,
                });
            });
            let stream = ReceiverStream::new(rx);
            return Ok(Sse::new(stream));
        }
        // If athena service not configured, fall through to direct SDK path.
    }

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
