//! Domain-focused API endpoint modules.
//!
//! Each sub-module owns a single responsibility area.
//! Shared types and the loading guard live here in mod.rs.

mod agent_groups;
mod agents;
mod athena_query;
mod compute;
mod connections;
pub(crate) mod doc;
pub mod embedding;
mod graph;
mod health;
pub(crate) mod prompts;
mod query;
pub(crate) mod stille_post;
pub(crate) mod telemetry;
pub(crate) mod villa;

use axum::Json;
use serde::Serialize;

use crate::state::{AppState, LoadingStatus};

// ── Shared types ─────────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct NotReadyResponse {
    pub error: &'static str,
    #[schema(value_type = Object)]
    pub loading: LoadingStatus,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
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

pub use health::{health, loading, stats, queue_status, scheduler_metrics};
pub use graph::{graph_nodes, graph_node_by_id, graph_force};
pub use compute::{
    compute_pagerank, compute_communities, compute_degrees,
    compute_patterns, compute_cooccurrence, compute_trends, compute_anomalies,
};
pub use query::query;
pub use agents::{
    agents_list, agents_execute, agents_chat,
    agents_get, agents_create, agents_update, agents_delete, agents_reload,
    bundeswehr_overview,
    skills_list, skills_get, skills_create, skills_update, skills_delete,
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
pub use stille_post::{
    sp_pipelines_list, sp_pipelines_create, sp_pipelines_get,
    sp_pipelines_update, sp_pipelines_delete,
    sp_data_sources_list, sp_data_sources_create, sp_data_sources_get,
    sp_data_sources_update, sp_data_sources_delete, sp_data_sources_test,
    sp_data_sources_upload,
    sp_deliveries_list, sp_deliveries_create, sp_deliveries_update,
    sp_deliveries_delete, sp_deliveries_test,
    sp_schedules_list, sp_schedules_create, sp_schedules_update, sp_schedules_delete,
    sp_agents_list, sp_agents_create, sp_agents_get, sp_agents_update, sp_agents_delete,
    sp_runs_list, sp_runs_get, sp_runs_create, sp_runs_delete,
    sp_reports_list, sp_reports_get,
    sp_export, sp_import,
};
pub use telemetry::{telemetry_events, telemetry_stats, telemetry_overview};
pub use agent_groups::{
    agent_groups_list, agent_groups_create, agent_groups_update, agent_groups_delete,
    agent_groups_add_agent, agent_groups_remove_agent,
};
pub use prompts::{prompts_list, prompts_get, prompts_update};
pub use villa::suggest::suggest as villa_suggest;
