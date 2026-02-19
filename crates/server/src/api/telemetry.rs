//! Telemetry endpoints for per-agent execution metrics.
//!
//! SRP: agent telemetry queries and aggregated stats.

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use stupid_agent::telemetry_store::{TelemetryEvent, TelemetryStats};

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Query params ────────────────────────────────────────────────

#[derive(Deserialize, utoipa::IntoParams)]
pub struct TelemetryQueryParams {
    /// Maximum number of events to return (default 50).
    pub limit: Option<usize>,
    /// Start of time range (ISO 8601 datetime).
    pub from: Option<String>,
    /// End of time range (ISO 8601 datetime).
    pub to: Option<String>,
}

// ── Response types ──────────────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct TelemetryEventsResponse {
    pub agent_name: String,
    pub count: usize,
    #[schema(value_type = Vec<Object>)]
    pub events: Vec<TelemetryEvent>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TelemetryStatsResponse {
    #[schema(value_type = Object)]
    pub stats: TelemetryStats,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TelemetryOverviewResponse {
    pub agent_count: usize,
    #[schema(value_type = Vec<Object>)]
    pub agents: Vec<TelemetryStats>,
}

// ── Handlers ────────────────────────────────────────────────────

/// Get telemetry events for an agent
///
/// Returns recent execution events. Supports optional `limit`, `from`, and `to`
/// query parameters for pagination and time-range filtering.
#[utoipa::path(
    get,
    path = "/api/telemetry/{agent_name}",
    tag = "Telemetry",
    params(
        ("agent_name" = String, Path, description = "Agent name"),
        TelemetryQueryParams,
    ),
    responses(
        (status = 200, description = "Telemetry events for the agent", body = TelemetryEventsResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn telemetry_events(
    State(state): State<Arc<AppState>>,
    Path(agent_name): Path<String>,
    Query(params): Query<TelemetryQueryParams>,
) -> Result<Json<TelemetryEventsResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.telemetry_store.read().await;

    let events = if params.from.is_some() || params.to.is_some() {
        let from = params
            .from
            .as_deref()
            .map(|s| s.parse::<chrono::DateTime<chrono::Utc>>())
            .transpose()
            .map_err(|e| bad_request(format!("invalid 'from' datetime: {e}")))?
            .unwrap_or(chrono::DateTime::<chrono::Utc>::MIN_UTC);
        let to = params
            .to
            .as_deref()
            .map(|s| s.parse::<chrono::DateTime<chrono::Utc>>())
            .transpose()
            .map_err(|e| bad_request(format!("invalid 'to' datetime: {e}")))?
            .unwrap_or_else(chrono::Utc::now);

        store
            .events_in_range(&agent_name, from, to)
            .map_err(|e| internal_error(e.to_string()))?
    } else {
        let limit = params.limit.unwrap_or(50);
        store
            .events_for_agent(&agent_name, limit)
            .map_err(|e| internal_error(e.to_string()))?
    };

    Ok(Json(TelemetryEventsResponse {
        agent_name,
        count: events.len(),
        events,
    }))
}

/// Get aggregated stats for an agent
///
/// Returns computed metrics: success/error/timeout counts, avg and p95 latency,
/// total token usage, and error rate.
#[utoipa::path(
    get,
    path = "/api/telemetry/{agent_name}/stats",
    tag = "Telemetry",
    params(
        ("agent_name" = String, Path, description = "Agent name"),
    ),
    responses(
        (status = 200, description = "Aggregated telemetry stats", body = TelemetryStatsResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn telemetry_stats(
    State(state): State<Arc<AppState>>,
    Path(agent_name): Path<String>,
) -> Result<Json<TelemetryStatsResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.telemetry_store.read().await;
    let stats = store
        .stats_for_agent(&agent_name)
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(TelemetryStatsResponse { stats }))
}

/// Get telemetry overview for all agents
///
/// Returns aggregated stats for every agent that has telemetry data,
/// sorted by total executions descending.
#[utoipa::path(
    get,
    path = "/api/telemetry/overview",
    tag = "Telemetry",
    responses(
        (status = 200, description = "Overview of all agent telemetry", body = TelemetryOverviewResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn telemetry_overview(
    State(state): State<Arc<AppState>>,
) -> Result<Json<TelemetryOverviewResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.telemetry_store.read().await;
    let agents = store
        .overview()
        .map_err(|e| internal_error(e.to_string()))?;
    Ok(Json(TelemetryOverviewResponse {
        agent_count: agents.len(),
        agents,
    }))
}

// ── Error helpers ───────────────────────────────────────────────

fn bad_request(
    msg: String,
) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::BAD_REQUEST,
        Json(QueryErrorResponse { error: msg }),
    )
}

fn internal_error(
    msg: String,
) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    (
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        Json(QueryErrorResponse { error: msg }),
    )
}
