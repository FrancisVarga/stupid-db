//! Request/response types and shared helpers for agent endpoints.

use axum::Json;
use serde::{Deserialize, Serialize};

use stupid_agent::yaml_schema::{AgentYamlConfig, ProviderConfig};

use crate::state::AppState;

use super::super::QueryErrorResponse;

// ── Agent types ──────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateAgentRequest {
    pub name: String,
    pub description: Option<String>,
    pub tier: Option<String>,
    pub tags: Option<Vec<String>>,
    pub group: Option<String>,
    #[schema(value_type = Object)]
    pub provider: serde_json::Value,
    pub system_prompt: Option<String>,
    pub skills: Option<Vec<SkillRequest>>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SkillRequest {
    pub name: String,
    pub prompt: String,
}

impl CreateAgentRequest {
    /// Convert the API request into an AgentYamlConfig.
    pub(super) fn into_yaml_config(self) -> Result<AgentYamlConfig, String> {
        let provider: ProviderConfig = serde_json::from_value(self.provider)
            .map_err(|e| format!("invalid provider config: {}", e))?;

        let tier = match self.tier.as_deref() {
            Some("Architect" | "architect") => stupid_agent::AgentTier::Architect,
            Some("Lead" | "lead") => stupid_agent::AgentTier::Lead,
            _ => stupid_agent::AgentTier::Specialist,
        };

        let skills = self
            .skills
            .unwrap_or_default()
            .into_iter()
            .map(|s| stupid_agent::yaml_schema::SkillConfig {
                name: s.name,
                prompt: s.prompt,
            })
            .collect();

        Ok(AgentYamlConfig {
            name: self.name,
            description: self.description.unwrap_or_default(),
            tier,
            tags: self.tags.unwrap_or_default(),
            group: self.group,
            provider,
            execution: Default::default(),
            system_prompt: self.system_prompt.unwrap_or_default(),
            skills,
        })
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AgentExecuteRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub context: serde_json::Value,
}

// ── Team types ───────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct TeamExecuteRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    #[schema(value_type = String)]
    pub strategy: stupid_agent::TeamStrategy,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub context: serde_json::Value,
}

pub(super) fn default_strategy() -> stupid_agent::TeamStrategy {
    stupid_agent::TeamStrategy::FullHierarchy
}

// ── Session types ────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionCreateRequest {
    pub name: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionUpdateRequest {
    pub name: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionExecuteAgentRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

pub(super) fn default_max_history() -> usize {
    10
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SessionExecuteResponse<T: Serialize> {
    #[schema(value_type = Object)]
    pub session: stupid_agent::session::SessionSummary,
    #[schema(value_type = Object)]
    pub response: T,
}

#[derive(Deserialize, utoipa::ToSchema)]
#[allow(dead_code)]
pub struct SessionExecuteTeamRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    #[schema(value_type = String)]
    pub strategy: stupid_agent::TeamStrategy,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionExecuteRequest {
    pub task: String,
    #[serde(default)]
    #[schema(value_type = Object)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SessionStreamRequest {
    pub task: String,
    pub system_prompt: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

pub(super) fn default_max_iterations() -> usize {
    10
}

// ── Shared helpers ───────────────────────────────────────────

pub(super) fn require_agent_store(
    state: &AppState,
) -> Result<&std::sync::Arc<stupid_agent::AgentStore>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    state.agent_store.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent store not configured. Set AGENTS_DIR.".into(),
            }),
        )
    })
}

/// Map an eisenbahn error to an HTTP error response for agent endpoints.
pub(super) fn eb_agent_error(e: stupid_eisenbahn::EisenbahnError) -> (axum::http::StatusCode, Json<QueryErrorResponse>) {
    let status = match &e {
        stupid_eisenbahn::EisenbahnError::Timeout(_) => axum::http::StatusCode::GATEWAY_TIMEOUT,
        _ => axum::http::StatusCode::BAD_GATEWAY,
    };
    (status, Json(QueryErrorResponse { error: e.to_string() }))
}

/// Parse a status string from eisenbahn into an ExecutionStatus.
pub(super) fn parse_execution_status(s: &str) -> stupid_agent::ExecutionStatus {
    match s {
        "success" => stupid_agent::ExecutionStatus::Success,
        "timeout" => stupid_agent::ExecutionStatus::Timeout,
        "partial" => stupid_agent::ExecutionStatus::Partial,
        _ => stupid_agent::ExecutionStatus::Error,
    }
}
