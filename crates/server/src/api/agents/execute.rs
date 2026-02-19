//! Agent and team execution endpoints: execute, chat, team execute, team strategies.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream;
use std::convert::Infallible;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::{
    eb_agent_error, parse_execution_status,
    AgentExecuteRequest, TeamExecuteRequest,
};

/// Execute an agent with a task
///
/// Runs the named agent against the provided task and optional context.
#[utoipa::path(
    post,
    path = "/agents/execute",
    tag = "Agents",
    request_body = AgentExecuteRequest,
    responses(
        (status = 200, description = "Agent execution result", body = Object),
        (status = 400, description = "Bad request", body = QueryErrorResponse),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<Json<stupid_agent::AgentResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::AgentServiceRequest::Execute {
            agent_name: req.agent_name.clone(),
            task: req.task.clone(),
            context: req.context.clone(),
        };
        let resp = eb
            .agent_execute(svc_req, Duration::from_secs(60))
            .await
            .map_err(|e| eb_agent_error(e))?;
        return Ok(Json(stupid_agent::AgentResponse {
            agent_name: req.agent_name,
            output: resp.output,
            status: parse_execution_status(&resp.status),
            execution_time_ms: resp.elapsed_ms,
            tokens_used: None,
        }));
    }

    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    let result = executor
        .execute(&req.agent_name, &req.task, context)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

    Ok(Json(result))
}

/// Stream agent chat response
///
/// Returns Server-Sent Events (SSE) with event types: agent_response, error, done
#[utoipa::path(
    post,
    path = "/agents/chat",
    tag = "Agents",
    request_body = AgentExecuteRequest,
    responses(
        (status = 200, description = "SSE stream of agent responses", content_type = "text/event-stream"),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn agents_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::AgentServiceRequest::Execute {
            agent_name: req.agent_name.clone(),
            task: req.task.clone(),
            context: req.context.clone(),
        };
        let result = eb.agent_execute(svc_req, Duration::from_secs(60)).await;
        let events = match result {
            Ok(resp) => {
                let agent_resp = stupid_agent::AgentResponse {
                    agent_name: req.agent_name,
                    output: resp.output,
                    status: parse_execution_status(&resp.status),
                    execution_time_ms: resp.elapsed_ms,
                    tokens_used: None,
                };
                let data = serde_json::to_string(&agent_resp).unwrap_or_default();
                vec![
                    Ok(Event::default().event("agent_response").data(data)),
                    Ok(Event::default().event("done").data("[DONE]")),
                ]
            }
            Err(e) => {
                vec![Ok(Event::default()
                    .event("error")
                    .data(serde_json::json!({"error": e.to_string()}).to_string()))]
            }
        };
        return Ok(Sse::new(stream::iter(events)));
    }

    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    // Execute agent and stream the response
    let result = executor
        .execute(&req.agent_name, &req.task, context)
        .await;

    let events = match result {
        Ok(response) => {
            let data = serde_json::to_string(&response).unwrap_or_default();
            vec![
                Ok(Event::default().event("agent_response").data(data)),
                Ok(Event::default().event("done").data("[DONE]")),
            ]
        }
        Err(e) => {
            vec![
                Ok(Event::default()
                    .event("error")
                    .data(serde_json::json!({"error": e.to_string()}).to_string())),
            ]
        }
    };

    Ok(Sse::new(stream::iter(events)))
}

// ── Team endpoints ─────────────────────────────────────────────

/// Execute a team of agents
///
/// Runs all agents as a team using the specified strategy (e.g., FullHierarchy, Parallel).
#[utoipa::path(
    post,
    path = "/teams/execute",
    tag = "Teams",
    request_body = TeamExecuteRequest,
    responses(
        (status = 200, description = "Team execution result", body = Object),
        (status = 503, description = "Agent system not configured", body = QueryErrorResponse)
    )
)]
pub async fn teams_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TeamExecuteRequest>,
) -> Result<Json<stupid_agent::TeamResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    // Route through eisenbahn if available.
    if let Some(ref eb) = state.eisenbahn {
        let svc_req = stupid_eisenbahn::services::AgentServiceRequest::TeamExecute {
            task: req.task.clone(),
            strategy: format!("{:?}", req.strategy).to_lowercase(),
            context: req.context.clone(),
        };
        let resp = eb
            .agent_execute(svc_req, Duration::from_secs(60))
            .await
            .map_err(|e| eb_agent_error(e))?;
        // Convert team_outputs JSON array into HashMap<String, String>
        let outputs: std::collections::HashMap<String, String> = resp
            .team_outputs
            .unwrap_or_default()
            .into_iter()
            .filter_map(|v| {
                let agent = v.get("agent").and_then(|a| a.as_str())?.to_string();
                let output = v.get("output").and_then(|o| o.as_str()).unwrap_or("").to_string();
                Some((agent, output))
            })
            .collect();
        return Ok(Json(stupid_agent::TeamResponse {
            task: req.task,
            outputs,
            status: parse_execution_status(&resp.status),
            execution_time_ms: resp.elapsed_ms,
            agents_used: Vec::new(),
            strategy: req.strategy,
        }));
    }

    let executor = state.agent_executor.as_ref().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agent system not configured.".into(),
            }),
        )
    })?;

    let context = if req.context.is_null() {
        None
    } else {
        Some(&req.context)
    };

    let result = stupid_agent::TeamExecutor::execute(
        executor,
        &req.task,
        req.strategy,
        context,
    )
    .await;

    Ok(Json(result))
}

/// List available team strategies
///
/// Returns all supported team execution strategies.
#[utoipa::path(
    get,
    path = "/teams/strategies",
    tag = "Teams",
    responses(
        (status = 200, description = "Available team strategies", body = Object)
    )
)]
pub async fn teams_strategies() -> Json<serde_json::Value> {
    let strategies = stupid_agent::TeamExecutor::strategies();
    Json(serde_json::json!({ "strategies": strategies }))
}
