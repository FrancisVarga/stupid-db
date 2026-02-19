//! Session execution endpoints: execute-agent, execute-team, execute (direct).

use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path, State};
use axum::Json;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::{
    eb_agent_error, parse_execution_status,
    SessionExecuteAgentRequest, SessionExecuteTeamRequest,
    SessionExecuteRequest, SessionExecuteResponse,
};

/// Execute an agent within a session
///
/// Runs the named agent within a session context, persisting both the user
/// message and the agent response to the session history.
#[utoipa::path(
    post,
    path = "/sessions/{id}/execute-agent",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = SessionExecuteAgentRequest,
    responses(
        (status = 200, description = "Agent execution result with session summary", body = Object),
        (status = 400, description = "Bad request", body = QueryErrorResponse),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn sessions_execute_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionExecuteAgentRequest>,
) -> Result<Json<SessionExecuteResponse<stupid_agent::AgentResponse>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {

    // Append user message
    let user_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::User,
        content: req.task.clone(),
        timestamp: chrono::Utc::now(),
        agent_name: None,
        status: None,
        execution_time_ms: None,
        team_outputs: None,
        agents_used: None,
        strategy: None,
    };

    {
        let store = state.session_store.write().await;
        store.append_message(&id, user_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append user message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?;
    }

    // Load session history for context
    let history = {
        let store = state.session_store.read().await;
        store.get(&id).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to read session: {}", e),
                }),
            )
        })?.map(|s| s.messages).unwrap_or_default()
    };

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    // Execute: route through eisenbahn if available, else direct call.
    let result = if let Some(ref eb) = state.eisenbahn {
        let history_json: Vec<serde_json::Value> = history
            .iter()
            .map(|m| serde_json::json!({ "role": format!("{:?}", m.role).to_lowercase(), "content": m.content }))
            .collect();
        let svc_req = stupid_eisenbahn::services::AgentServiceRequest::ExecuteWithHistory {
            agent_name: req.agent_name.clone(),
            task: req.task.clone(),
            history: history_json,
            context: req.context.clone(),
            max_history: req.max_history,
        };
        let resp = eb
            .agent_execute(svc_req, Duration::from_secs(60))
            .await
            .map_err(|e| eb_agent_error(e))?;
        stupid_agent::AgentResponse {
            agent_name: req.agent_name,
            output: resp.output,
            status: parse_execution_status(&resp.status),
            execution_time_ms: resp.elapsed_ms,
            tokens_used: None,
        }
    } else {
        let executor = state.agent_executor.as_ref().ok_or_else(|| {
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(QueryErrorResponse {
                    error: "Agent system not configured.".into(),
                }),
            )
        })?;
        executor
            .execute_with_history(&req.agent_name, &req.task, &history, context, req.max_history)
            .await
            .map_err(|e| {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(QueryErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?
    };

    // Append agent response
    let agent_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::Agent,
        content: result.output.clone(),
        timestamp: chrono::Utc::now(),
        agent_name: Some(result.agent_name.clone()),
        status: Some(format!("{:?}", result.status).to_lowercase()),
        execution_time_ms: Some(result.execution_time_ms),
        team_outputs: None,
        agents_used: None,
        strategy: None,
    };

    let session = {
        let store = state.session_store.write().await;
        store.append_message(&id, agent_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append agent message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?
    };

    Ok(Json(SessionExecuteResponse {
        session: stupid_agent::session::SessionSummary::from(&session),
        response: result,
    }))
}

/// Execute a team within a session
///
/// Runs all agents as a team within a session context, persisting both the
/// user message and the team response to the session history.
#[utoipa::path(
    post,
    path = "/sessions/{id}/execute-team",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = SessionExecuteTeamRequest,
    responses(
        (status = 200, description = "Team execution result with session summary", body = Object),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn sessions_execute_team(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionExecuteTeamRequest>,
) -> Result<Json<SessionExecuteResponse<stupid_agent::TeamResponse>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    // Append user message
    let user_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::User,
        content: req.task.clone(),
        timestamp: chrono::Utc::now(),
        agent_name: None,
        status: None,
        execution_time_ms: None,
        team_outputs: None,
        agents_used: None,
        strategy: None,
    };

    {
        let store = state.session_store.write().await;
        store.append_message(&id, user_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append user message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?;
    }

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    // Execute team
    let result = stupid_agent::TeamExecutor::execute(
        executor,
        &req.task,
        req.strategy,
        context,
    )
    .await;

    // Append team response
    let team_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::Team,
        content: format!("Team execution: {} agents", result.agents_used.len()),
        timestamp: chrono::Utc::now(),
        agent_name: None,
        status: Some(format!("{:?}", result.status).to_lowercase()),
        execution_time_ms: Some(result.execution_time_ms),
        team_outputs: Some(result.outputs.clone()),
        agents_used: Some(result.agents_used.clone()),
        strategy: Some(format!("{:?}", result.strategy).to_lowercase()),
    };

    let session = {
        let store = state.session_store.write().await;
        store.append_message(&id, team_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append team message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?
    };

    Ok(Json(SessionExecuteResponse {
        session: stupid_agent::session::SessionSummary::from(&session),
        response: result,
    }))
}

/// Execute directly against the LLM within a session
///
/// Runs a task directly against the LLM without agent selection, within a
/// session context. Both the user message and response are persisted.
#[utoipa::path(
    post,
    path = "/sessions/{id}/execute",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = SessionExecuteRequest,
    responses(
        (status = 200, description = "Direct execution result with session summary", body = Object),
        (status = 400, description = "Bad request", body = QueryErrorResponse),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn sessions_execute(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionExecuteRequest>,
) -> Result<Json<SessionExecuteResponse<stupid_agent::AgentResponse>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {

    // Append user message
    let user_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::User,
        content: req.task.clone(),
        timestamp: chrono::Utc::now(),
        agent_name: None,
        status: None,
        execution_time_ms: None,
        team_outputs: None,
        agents_used: None,
        strategy: None,
    };

    {
        let store = state.session_store.write().await;
        store.append_message(&id, user_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append user message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?;
    }

    // Load session history for context
    let history = {
        let store = state.session_store.read().await;
        store.get(&id).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to read session: {}", e),
                }),
            )
        })?.map(|s| s.messages).unwrap_or_default()
    };

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    // Execute: route through eisenbahn if available, else direct call.
    let result = if let Some(ref eb) = state.eisenbahn {
        let history_json: Vec<serde_json::Value> = history
            .iter()
            .map(|m| serde_json::json!({ "role": format!("{:?}", m.role).to_lowercase(), "content": m.content }))
            .collect();
        let svc_req = stupid_eisenbahn::services::AgentServiceRequest::ExecuteDirect {
            task: req.task.clone(),
            history: history_json,
            context: req.context.clone(),
            max_history: req.max_history,
        };
        let resp = eb
            .agent_execute(svc_req, Duration::from_secs(60))
            .await
            .map_err(|e| eb_agent_error(e))?;
        stupid_agent::AgentResponse {
            agent_name: "assistant".to_string(),
            output: resp.output,
            status: parse_execution_status(&resp.status),
            execution_time_ms: resp.elapsed_ms,
            tokens_used: None,
        }
    } else {
        let executor = state.agent_executor.as_ref().ok_or_else(|| {
            (
                axum::http::StatusCode::SERVICE_UNAVAILABLE,
                Json(QueryErrorResponse {
                    error: "Agent system not configured.".into(),
                }),
            )
        })?;
        executor
            .execute_direct(&req.task, &history, context, req.max_history)
            .await
            .map_err(|e| {
                (
                    axum::http::StatusCode::BAD_REQUEST,
                    Json(QueryErrorResponse {
                        error: e.to_string(),
                    }),
                )
            })?
    };

    // Append assistant response
    let agent_msg = stupid_agent::session::SessionMessage {
        id: uuid::Uuid::new_v4().to_string(),
        role: stupid_agent::session::SessionMessageRole::Agent,
        content: result.output.clone(),
        timestamp: chrono::Utc::now(),
        agent_name: Some("assistant".to_string()),
        status: Some(format!("{:?}", result.status).to_lowercase()),
        execution_time_ms: Some(result.execution_time_ms),
        team_outputs: None,
        agents_used: None,
        strategy: None,
    };

    let session = {
        let store = state.session_store.write().await;
        store.append_message(&id, agent_msg).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to append agent message: {}", e),
                }),
            )
        })?.ok_or_else(|| {
            (
                axum::http::StatusCode::NOT_FOUND,
                Json(QueryErrorResponse {
                    error: format!("Session not found: {}", id),
                }),
            )
        })?
    };

    Ok(Json(SessionExecuteResponse {
        session: stupid_agent::session::SessionSummary::from(&session),
        response: result,
    }))
}
