//! Agent CRUD endpoints.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use sqlx::types::Uuid;

use crate::state::AppState;

use super::common::{internal_error, not_found, require_pg, ApiResult};

// ── Types ────────────────────────────────────────────────────────

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct SpAgent {
    pub id: Uuid,
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub model: String,
    pub skills_config: serde_json::Value,
    pub mcp_servers_config: serde_json::Value,
    pub tools_config: serde_json::Value,
    pub template_id: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: Option<String>,
    pub system_prompt: String,
    pub model: Option<String>,
    pub skills_config: Option<serde_json::Value>,
    pub mcp_servers_config: Option<serde_json::Value>,
    pub tools_config: Option<serde_json::Value>,
    pub template_id: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentRequest {
    pub name: Option<String>,
    pub description: Option<String>,
    pub system_prompt: Option<String>,
    pub model: Option<String>,
    pub skills_config: Option<serde_json::Value>,
    pub mcp_servers_config: Option<serde_json::Value>,
    pub tools_config: Option<serde_json::Value>,
    pub template_id: Option<String>,
}

// ── Handlers ─────────────────────────────────────────────────────

/// GET /sp/agents -- list all agents.
pub async fn sp_agents_list(
    State(state): State<Arc<AppState>>,
) -> ApiResult<Json<Vec<SpAgent>>> {
    let pool = require_pg(&state)?;
    let rows = sqlx::query_as::<_, SpAgent>(
        "SELECT id, name, description, system_prompt, model, \
                skills_config, mcp_servers_config, tools_config, template_id, \
                created_at, updated_at \
         FROM sp_agents ORDER BY created_at DESC",
    )
    .fetch_all(pool)
    .await
    .map_err(internal_error)?;
    Ok(Json(rows))
}

/// POST /sp/agents -- create a new agent.
pub async fn sp_agents_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<CreateAgentRequest>,
) -> ApiResult<(axum::http::StatusCode, Json<SpAgent>)> {
    let pool = require_pg(&state)?;
    let empty_arr = serde_json::json!([]);
    let row = sqlx::query_as::<_, SpAgent>(
        "INSERT INTO sp_agents \
            (name, description, system_prompt, model, skills_config, \
             mcp_servers_config, tools_config, template_id) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8) \
         RETURNING *",
    )
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.system_prompt)
    .bind(req.model.as_deref().unwrap_or("claude-sonnet-4-6"))
    .bind(req.skills_config.as_ref().unwrap_or(&empty_arr))
    .bind(req.mcp_servers_config.as_ref().unwrap_or(&empty_arr))
    .bind(req.tools_config.as_ref().unwrap_or(&empty_arr))
    .bind(&req.template_id)
    .fetch_one(pool)
    .await
    .map_err(internal_error)?;
    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

/// GET /sp/agents/:id -- get agent by ID.
pub async fn sp_agents_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<SpAgent>> {
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpAgent>("SELECT * FROM sp_agents WHERE id = $1")
        .bind(id)
        .fetch_optional(pool)
        .await
        .map_err(internal_error)?
        .ok_or_else(|| not_found("Agent", id))?;
    Ok(Json(row))
}

/// PUT /sp/agents/:id -- update an agent.
pub async fn sp_agents_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateAgentRequest>,
) -> ApiResult<Json<SpAgent>> {
    let pool = require_pg(&state)?;
    let row = sqlx::query_as::<_, SpAgent>(
        "UPDATE sp_agents SET \
            name = COALESCE($2, name), \
            description = COALESCE($3, description), \
            system_prompt = COALESCE($4, system_prompt), \
            model = COALESCE($5, model), \
            skills_config = COALESCE($6, skills_config), \
            mcp_servers_config = COALESCE($7, mcp_servers_config), \
            tools_config = COALESCE($8, tools_config), \
            template_id = COALESCE($9, template_id), \
            updated_at = now() \
         WHERE id = $1 \
         RETURNING *",
    )
    .bind(id)
    .bind(&req.name)
    .bind(&req.description)
    .bind(&req.system_prompt)
    .bind(&req.model)
    .bind(&req.skills_config)
    .bind(&req.mcp_servers_config)
    .bind(&req.tools_config)
    .bind(&req.template_id)
    .fetch_optional(pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| not_found("Agent", id))?;
    Ok(Json(row))
}

/// DELETE /sp/agents/:id -- delete an agent.
pub async fn sp_agents_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<Uuid>,
) -> ApiResult<axum::http::StatusCode> {
    let pool = require_pg(&state)?;
    let result = sqlx::query("DELETE FROM sp_agents WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(internal_error)?;
    if result.rows_affected() == 0 {
        return Err(not_found("Agent", id));
    }
    Ok(axum::http::StatusCode::NO_CONTENT)
}
