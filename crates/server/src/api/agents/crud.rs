//! Agent CRUD endpoints: list, get, create, update, delete, reload.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::{require_agent_store, CreateAgentRequest};

// ── Agent endpoints ────────────────────────────────────────────

/// List all configured agents
///
/// Returns agent metadata for all agents loaded from the agents directory.
/// If AgentStore is available, uses it for richer data including tags and group.
#[utoipa::path(
    get,
    path = "/agents/list",
    tag = "Agents",
    responses(
        (status = 200, description = "List of configured agents", body = Object),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agents_list(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
    // Prefer AgentStore (richer data) over executor.agents
    if let Some(ref store) = state.agent_store {
        let agents = store.list().await;
        let infos: Vec<serde_json::Value> = agents
            .iter()
            .map(|a| {
                serde_json::json!({
                    "name": a.name,
                    "description": a.description,
                    "tier": a.tier,
                    "tags": a.tags,
                    "group": a.group,
                    "provider_type": a.provider.provider_type(),
                    "model": a.provider.model(),
                })
            })
            .collect();
        return Json(serde_json::json!({ "agents": infos }));
    }

    match state.agent_executor.as_ref() {
        Some(executor) => {
            let agents = stupid_agent::config::agents_to_info(&executor.agents);
            Json(serde_json::json!({ "agents": agents }))
        }
        None => Json(serde_json::json!({
            "agents": [],
            "error": "Agent system not configured. Set AGENTS_DIR in config."
        })),
    }
}

/// Get a single agent by name
///
/// Returns the full YAML configuration for the named agent.
#[utoipa::path(
    get,
    path = "/api/agents/{name}",
    tag = "Agents",
    params(
        ("name" = String, Path, description = "Agent name")
    ),
    responses(
        (status = 200, description = "Agent configuration", body = Object),
        (status = 404, description = "Agent not found", body = QueryErrorResponse),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_get(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = require_agent_store(&state)?;
    match store.get(&name).await {
        Some(config) => Ok(Json(serde_json::to_value(&config).unwrap_or_default())),
        None => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Agent not found: {}", name),
            }),
        )),
    }
}

/// Create a new agent
///
/// Creates a new agent configuration and writes it to disk as a YAML file.
#[utoipa::path(
    post,
    path = "/api/agents",
    tag = "Agents",
    request_body = CreateAgentRequest,
    responses(
        (status = 201, description = "Agent created", body = Object),
        (status = 400, description = "Invalid request", body = QueryErrorResponse),
        (status = 409, description = "Agent already exists", body = QueryErrorResponse),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<(axum::http::StatusCode, Json<serde_json::Value>), (axum::http::StatusCode, Json<QueryErrorResponse>)>
{
    let store = require_agent_store(&state)?;
    let config = req.into_yaml_config().map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(QueryErrorResponse { error: e }),
        )
    })?;

    let created = store.create(config).await.map_err(|e| {
        let msg = e.to_string();
        let status = if msg.contains("already exists") {
            axum::http::StatusCode::CONFLICT
        } else {
            axum::http::StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: msg }))
    })?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(serde_json::to_value(&created).unwrap_or_default()),
    ))
}

/// Update an existing agent
///
/// Updates the agent configuration and writes changes back to disk.
#[utoipa::path(
    put,
    path = "/api/agents/{name}",
    tag = "Agents",
    params(
        ("name" = String, Path, description = "Agent name")
    ),
    request_body = CreateAgentRequest,
    responses(
        (status = 200, description = "Agent updated", body = Object),
        (status = 400, description = "Invalid request", body = QueryErrorResponse),
        (status = 404, description = "Agent not found", body = QueryErrorResponse),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_update(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<CreateAgentRequest>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = require_agent_store(&state)?;
    let config = req.into_yaml_config().map_err(|e| {
        (
            axum::http::StatusCode::BAD_REQUEST,
            Json(QueryErrorResponse { error: e }),
        )
    })?;

    match store.update(&name, config).await {
        Ok(Some(updated)) => Ok(Json(serde_json::to_value(&updated).unwrap_or_default())),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Agent not found: {}", name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Delete an agent
///
/// Removes the agent from the store and deletes its YAML file from disk.
#[utoipa::path(
    delete,
    path = "/api/agents/{name}",
    tag = "Agents",
    params(
        ("name" = String, Path, description = "Agent name")
    ),
    responses(
        (status = 200, description = "Agent deleted", body = Object),
        (status = 404, description = "Agent not found", body = QueryErrorResponse),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = require_agent_store(&state)?;
    match store.delete(&name).await {
        Ok(true) => Ok(Json(serde_json::json!({ "deleted": name }))),
        Ok(false) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Agent not found: {}", name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Hot-reload agents from disk
///
/// Re-scans the agents directory and refreshes the in-memory store.
#[utoipa::path(
    post,
    path = "/api/agents/reload",
    tag = "Agents",
    responses(
        (status = 200, description = "Agents reloaded", body = Object),
        (status = 503, description = "Agent store not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_reload(
    State(state): State<Arc<AppState>>,
) -> Result<Json<serde_json::Value>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = require_agent_store(&state)?;
    match store.reload().await {
        Ok(count) => Ok(Json(serde_json::json!({
            "reloaded": true,
            "agent_count": count,
        }))),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Reload failed: {}", e),
            }),
        )),
    }
}
