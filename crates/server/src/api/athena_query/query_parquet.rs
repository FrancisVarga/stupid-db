//! Parquet download endpoint for Athena query results.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

use super::helpers::eb_athena_error;
use super::types::AthenaQueryRequest;
use crate::api::QueryErrorResponse;

/// Export Athena query results as Parquet
///
/// Uses the same query execution flow as the SSE endpoint but collects all
/// results into memory and returns a single Parquet file with proper types
/// and Zstd compression. The response includes Content-Disposition header
/// for browser download.
#[utoipa::path(
    post,
    path = "/athena-connections/{id}/query/parquet",
    tag = "Athena Queries",
    request_body = AthenaQueryRequest,
    params(("id" = String, Path, description = "Athena connection ID")),
    responses(
        (status = 200, description = "Parquet file", content_type = "application/octet-stream"),
        (status = 500, description = "Query error", body = QueryErrorResponse)
    )
)]
pub async fn athena_query_parquet(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<AthenaQueryRequest>,
) -> Result<
    axum::response::Response,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::AthenaServiceRequest::QueryParquet {
            connection_id: id.clone(),
            sql: req.sql.clone(),
            database: req.database.clone(),
        };
        let mut zmq_rx = eb
            .athena_query_stream(svc_req)
            .await
            .map_err(|e| eb_athena_error(e))?;

        // Collect all parquet bytes from the stream.
        let mut parquet_bytes: Option<Vec<u8>> = None;
        while let Some(result) = zmq_rx.recv().await {
            match result {
                Ok(msg) => {
                    if let Ok(resp) = msg.decode::<stupid_eisenbahn::services::AthenaServiceResponse>() {
                        match resp {
                            stupid_eisenbahn::services::AthenaServiceResponse::Parquet { data } => {
                                parquet_bytes = Some(data);
                            }
                            stupid_eisenbahn::services::AthenaServiceResponse::Error { message } => {
                                return Err((
                                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                                    Json(QueryErrorResponse { error: message }),
                                ));
                            }
                            _ => {} // Skip status/done chunks
                        }
                    }
                }
                Err(e) => {
                    return Err((
                        axum::http::StatusCode::BAD_GATEWAY,
                        Json(QueryErrorResponse { error: e.to_string() }),
                    ));
                }
            }
        }

        let bytes = parquet_bytes.ok_or_else(|| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse { error: "No parquet data received from service".into() }),
            )
        })?;

        let filename = format!("{}.parquet", uuid::Uuid::new_v4());
        return Ok(axum::response::Response::builder()
            .status(200)
            .header("Content-Type", "application/vnd.apache.parquet")
            .header("Content-Disposition", format!("attachment; filename=\"{filename}\""))
            .header("Content-Length", bytes.len().to_string())
            .body(axum::body::Body::from(bytes))
            .unwrap());
    }

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
