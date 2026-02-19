//! Skill CRUD endpoints for Bundeswehr skill management.
//!
//! SRP: standalone skill lifecycle (list, get, create, update, delete).

use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use stupid_agent::yaml_schema::SkillYamlConfig;

use crate::state::AppState;

use super::super::QueryErrorResponse;

// ── Query params ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SkillListParams {
    /// Optional search text to filter skills by name, description, or tags.
    pub search: Option<String>,
}

// ── Response types ──────────────────────────────────────────────

/// Compact skill info returned by the list endpoint.
#[derive(Debug, Serialize)]
pub struct SkillInfo {
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub version: String,
}

/// Full skill detail with usage information.
#[derive(Debug, Serialize)]
pub struct SkillDetail {
    pub name: String,
    pub description: String,
    pub prompt: String,
    pub tags: Vec<String>,
    pub version: String,
    pub used_by: Vec<String>,
}

/// Request body for creating or updating a skill.
#[derive(Debug, Deserialize)]
pub struct SkillRequest {
    pub name: String,
    pub description: Option<String>,
    pub prompt: String,
    pub tags: Option<Vec<String>>,
    pub version: Option<String>,
}

impl SkillRequest {
    fn into_yaml_config(self) -> SkillYamlConfig {
        SkillYamlConfig {
            name: self.name,
            description: self.description.unwrap_or_default(),
            prompt: self.prompt,
            tags: self.tags.unwrap_or_default(),
            version: self.version.unwrap_or_else(|| "1.0.0".to_string()),
        }
    }
}

#[derive(Serialize)]
pub struct SkillListResponse {
    skills: Vec<SkillInfo>,
    count: usize,
}

// ── Helpers ─────────────────────────────────────────────────────

/// Require skill_store to be configured, or return 503.
fn require_skill_store(
    state: &AppState,
) -> Result<&Arc<stupid_agent::SkillStore>, (StatusCode, Json<QueryErrorResponse>)> {
    state.skill_store.as_ref().ok_or_else(|| {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Skill store not configured.".into(),
            }),
        )
    })
}

/// Compute which agents reference a given skill name.
async fn compute_used_by(state: &AppState, skill_name: &str) -> Vec<String> {
    let Some(ref agent_store) = state.agent_store else {
        return Vec::new();
    };
    let agents = agent_store.list().await;
    agents
        .iter()
        .filter(|a| a.skill_refs.iter().any(|r| r == skill_name))
        .map(|a| a.name.clone())
        .collect()
}

fn skill_to_info(s: &SkillYamlConfig) -> SkillInfo {
    SkillInfo {
        name: s.name.clone(),
        description: s.description.clone(),
        tags: s.tags.clone(),
        version: s.version.clone(),
    }
}

fn skill_to_detail(s: &SkillYamlConfig, used_by: Vec<String>) -> SkillDetail {
    SkillDetail {
        name: s.name.clone(),
        description: s.description.clone(),
        prompt: s.prompt.clone(),
        tags: s.tags.clone(),
        version: s.version.clone(),
        used_by,
    }
}

fn matches_search(skill: &SkillYamlConfig, query: &str) -> bool {
    let q = query.to_lowercase();
    skill.name.to_lowercase().contains(&q)
        || skill.description.to_lowercase().contains(&q)
        || skill.tags.iter().any(|t| t.to_lowercase().contains(&q))
}

// ── Handlers ────────────────────────────────────────────────────

/// List all skills
///
/// Returns compact skill info for all standalone skills.
/// Supports optional `?search=text` to filter by name, description, or tags.
pub async fn skills_list(
    State(state): State<Arc<AppState>>,
    Query(params): Query<SkillListParams>,
) -> Result<Json<SkillListResponse>, (StatusCode, Json<QueryErrorResponse>)> {
    let store = require_skill_store(&state)?;
    let all = store.list().await;

    let skills: Vec<SkillInfo> = match &params.search {
        Some(q) if !q.is_empty() => all
            .iter()
            .filter(|s| matches_search(s, q))
            .map(skill_to_info)
            .collect(),
        _ => all.iter().map(skill_to_info).collect(),
    };

    let count = skills.len();
    Ok(Json(SkillListResponse { skills, count }))
}

/// Get a single skill by name
///
/// Returns full skill detail including prompt and `used_by` agent list.
pub async fn skills_get(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<SkillDetail>, (StatusCode, Json<QueryErrorResponse>)> {
    let store = require_skill_store(&state)?;
    match store.get(&name).await {
        Some(skill) => {
            let used_by = compute_used_by(&state, &name).await;
            Ok(Json(skill_to_detail(&skill, used_by)))
        }
        None => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Skill not found: {}", name),
            }),
        )),
    }
}

/// Create a new skill
///
/// Creates a new standalone skill and writes it to disk as a YAML file.
pub async fn skills_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SkillRequest>,
) -> Result<(StatusCode, Json<SkillDetail>), (StatusCode, Json<QueryErrorResponse>)> {
    let store = require_skill_store(&state)?;
    let config = req.into_yaml_config();

    let created = store.create(config).await.map_err(|e| {
        let msg = e.to_string();
        let status = if msg.contains("already exists") {
            StatusCode::CONFLICT
        } else {
            StatusCode::INTERNAL_SERVER_ERROR
        };
        (status, Json(QueryErrorResponse { error: msg }))
    })?;

    Ok((
        StatusCode::CREATED,
        Json(skill_to_detail(&created, Vec::new())),
    ))
}

/// Update an existing skill
///
/// Updates the skill configuration and writes changes back to disk.
pub async fn skills_update(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
    Json(req): Json<SkillRequest>,
) -> Result<Json<SkillDetail>, (StatusCode, Json<QueryErrorResponse>)> {
    let store = require_skill_store(&state)?;
    let config = req.into_yaml_config();

    match store.update(&name, config).await {
        Ok(Some(updated)) => {
            let used_by = compute_used_by(&state, &updated.name).await;
            Ok(Json(skill_to_detail(&updated, used_by)))
        }
        Ok(None) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Skill not found: {}", name),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}

/// Delete a skill
///
/// Removes the skill from the store and deletes its YAML file from disk.
pub async fn skills_delete(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<QueryErrorResponse>)> {
    let store = require_skill_store(&state)?;
    match store.delete(&name).await {
        Ok(true) => Ok(Json(serde_json::json!({ "deleted": name }))),
        Ok(false) => Err((
            StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Skill not found: {}", name),
            }),
        )),
        Err(e) => Err((
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: e.to_string(),
            }),
        )),
    }
}
