//! Query audit log endpoint for Athena connections.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;

use crate::credential_store::CredentialStore;
use crate::state::AppState;

/// Get query audit log for an Athena connection
///
/// Returns matching log entries (newest first) with cumulative and daily cost
/// summaries. Supports filtering by source, outcome, time range, SQL text,
/// and result limit.
#[utoipa::path(
    get,
    path = "/athena-connections/{id}/query-log",
    tag = "Athena Queries",
    params(
        ("id" = String, Path, description = "Athena connection ID"),
        ("source" = Option<String>, Query, description = "Filter by query source"),
        ("outcome" = Option<String>, Query, description = "Filter by outcome"),
        ("since" = Option<String>, Query, description = "ISO 8601 lower bound (inclusive)"),
        ("until" = Option<String>, Query, description = "ISO 8601 upper bound (exclusive)"),
        ("limit" = Option<u32>, Query, description = "Maximum entries to return (default 100)"),
        ("sql_contains" = Option<String>, Query, description = "Case-insensitive SQL substring match"),
    ),
    responses(
        (status = 200, description = "Query log entries with cost summary", body = Object),
        (status = 404, description = "Connection not found")
    )
)]
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
