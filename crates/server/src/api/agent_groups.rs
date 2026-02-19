//! Agent group CRUD endpoints.
//!
//! SRP: manage named groups of agents with membership operations.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;

use stupid_agent::group_store::AgentGroup;

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Request types ─────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateGroupRequest {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateGroupRequest {
    pub description: Option<String>,
    pub color: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AgentMemberRequest {
    pub agent_name: String,
}

// ── Endpoints ─────────────────────────────────────────────────

/// List all agent groups
#[utoipa::path(
    get,
    path = "/agent-groups",
    tag = "Agent Groups",
    responses(
        (status = 200, description = "List of agent groups", body = Object),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<AgentGroup>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list groups: {}", e),
            }),
        )
    })
}

/// Create a new agent group
#[utoipa::path(
    post,
    path = "/agent-groups",
    tag = "Agent Groups",
    request_body = CreateGroupRequest,
    responses(
        (status = 201, description = "Group created", body = Object),
        (status = 409, description = "Group already exists", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateGroupRequest>,
) -> Result<(axum::http::StatusCode, Json<AgentGroup>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.write().await;
    store
        .create(
            &req.name,
            req.description.as_deref().unwrap_or(""),
            req.color.as_deref().unwrap_or("#6366f1"),
        )
        .map(|g| (axum::http::StatusCode::CREATED, Json(g)))
        .map_err(|e| {
            let msg = e.to_string();
            let status = if msg.contains("already exists") {
                axum::http::StatusCode::CONFLICT
            } else {
                axum::http::StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(QueryErrorResponse { error: msg }))
        })
}

/// Update an agent group
#[utoipa::path(
    put,
    path = "/agent-groups/{name}",
    tag = "Agent Groups",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    request_body = UpdateGroupRequest,
    responses(
        (status = 200, description = "Group updated", body = Object),
        (status = 404, description = "Group not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_update(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<UpdateGroupRequest>,
) -> Result<Json<AgentGroup>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.write().await;
    match store.update(&name, req.description.as_deref(), req.color.as_deref()) {
        Ok(Some(group)) => Ok(Json(group)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Group not found: {}", name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to update group: {}", e),
            }),
        )),
    }
}

/// Delete an agent group
#[utoipa::path(
    delete,
    path = "/agent-groups/{name}",
    tag = "Agent Groups",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    responses(
        (status = 204, description = "Group deleted"),
        (status = 404, description = "Group not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.write().await;
    match store.delete(&name) {
        Ok(true) => Ok(axum::http::StatusCode::NO_CONTENT),
        Ok(false) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Group not found: {}", name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to delete group: {}", e),
            }),
        )),
    }
}

/// Add an agent to a group
#[utoipa::path(
    post,
    path = "/agent-groups/{name}/agents",
    tag = "Agent Groups",
    params(
        ("name" = String, Path, description = "Group name")
    ),
    request_body = AgentMemberRequest,
    responses(
        (status = 200, description = "Agent added to group", body = Object),
        (status = 404, description = "Group not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_add_agent(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<AgentMemberRequest>,
) -> Result<Json<AgentGroup>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.write().await;
    match store.add_agent(&name, &req.agent_name) {
        Ok(Some(group)) => Ok(Json(group)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Group not found: {}", name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to add agent: {}", e),
            }),
        )),
    }
}

/// Remove an agent from a group
#[utoipa::path(
    delete,
    path = "/agent-groups/{group_name}/{agent_name}",
    tag = "Agent Groups",
    params(
        ("group_name" = String, Path, description = "Group name"),
        ("agent_name" = String, Path, description = "Agent name to remove")
    ),
    responses(
        (status = 200, description = "Agent removed from group", body = Object),
        (status = 404, description = "Group not found", body = QueryErrorResponse),
        (status = 500, description = "Internal error", body = QueryErrorResponse)
    )
)]
pub async fn agent_groups_remove_agent(
    State(state): State<Arc<AppState>>,
    Path((group_name, agent_name)): Path<(String, String)>,
) -> Result<Json<AgentGroup>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.group_store.write().await;
    match store.remove_agent(&group_name, &agent_name) {
        Ok(Some(group)) => Ok(Json(group)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Group not found: {}", group_name),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to remove agent: {}", e),
            }),
        )),
    }
}
