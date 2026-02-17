//! Domain-focused API endpoint modules.
//!
//! Each sub-module owns a single responsibility area.
//! Shared types and the loading guard live here in mod.rs.

mod agents;
mod athena_query;
mod compute;
mod connections;
pub mod embedding;
mod graph;
mod health;
mod query;

use axum::Json;
use serde::Serialize;

use crate::state::{AppState, LoadingStatus};

// ── Shared types ─────────────────────────────────────────────────

#[derive(Serialize)]
pub struct NotReadyResponse {
    pub error: &'static str,
    pub loading: LoadingStatus,
}

#[derive(Serialize)]
pub struct QueryErrorResponse {
    pub error: String,
}

// ── Loading guard ────────────────────────────────────────────────

/// Return 503 with loading progress if data isn't ready yet.
pub(crate) async fn require_ready(
    state: &AppState,
) -> Result<(), (axum::http::StatusCode, Json<NotReadyResponse>)> {
    if state.loading.is_ready().await {
        return Ok(());
    }
    let status = state.loading.to_status().await;
    Err((
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        Json(NotReadyResponse {
            error: "Data is still loading. Check /loading for progress.",
            loading: status,
        }),
    ))
}

// ── Re-exports ───────────────────────────────────────────────────
// Preserves flat `api::foo` import paths used by main.rs route registration.

pub use health::{health, loading, stats, catalog, queue_status, scheduler_metrics};
pub use graph::{graph_nodes, graph_node_by_id, graph_force};
pub use compute::{
    compute_pagerank, compute_communities, compute_degrees,
    compute_patterns, compute_cooccurrence, compute_trends, compute_anomalies,
};
pub use query::query;
pub use agents::{
    agents_list, agents_execute, agents_chat,
    teams_execute, teams_strategies,
    sessions_list, sessions_create, sessions_get, sessions_update, sessions_delete,
    sessions_execute_agent, sessions_execute_team, sessions_execute,
    sessions_stream,
};
pub use connections::{
    connections_list, connections_add, connections_get,
    connections_update, connections_delete, connections_credentials,
    queue_connections_list, queue_connections_add, queue_connections_get,
    queue_connections_update, queue_connections_delete, queue_connections_credentials,
    athena_connections_list, athena_connections_add, athena_connections_get,
    athena_connections_update, athena_connections_delete, athena_connections_credentials,
};
pub use athena_query::{
    athena_query_sse, athena_query_parquet,
    athena_connections_schema, athena_connections_schema_refresh,
    athena_connections_query_log,
};
