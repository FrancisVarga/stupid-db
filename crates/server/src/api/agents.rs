//! Agent orchestration endpoints: single agent execution, team execution,
//! and session-based chat with history.
//!
//! SRP: agent/team lifecycle and session management.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use futures::stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;

use stupid_tool_runtime::conversation::{AssistantContent, ConversationMessage};
use stupid_tool_runtime::stream::StreamEvent;
use stupid_tool_runtime::tool::ToolContext;

use crate::state::AppState;

use super::QueryErrorResponse;

// ── Agent endpoints ────────────────────────────────────────────

pub async fn agents_list(
    State(state): State<Arc<AppState>>,
) -> Json<serde_json::Value> {
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

#[derive(Deserialize)]
pub struct AgentExecuteRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

pub async fn agents_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<Json<stupid_agent::AgentResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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

/// SSE streaming endpoint for agent chat.
pub async fn agents_chat(
    State(state): State<Arc<AppState>>,
    Json(req): Json<AgentExecuteRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
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

#[derive(Deserialize)]
pub struct TeamExecuteRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    pub strategy: stupid_agent::TeamStrategy,
    #[serde(default)]
    pub context: serde_json::Value,
}

fn default_strategy() -> stupid_agent::TeamStrategy {
    stupid_agent::TeamStrategy::FullHierarchy
}

pub async fn teams_execute(
    State(state): State<Arc<AppState>>,
    Json(req): Json<TeamExecuteRequest>,
) -> Result<Json<stupid_agent::TeamResponse>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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

pub async fn teams_strategies() -> Json<serde_json::Value> {
    let strategies = stupid_agent::TeamExecutor::strategies();
    Json(serde_json::json!({ "strategies": strategies }))
}

// ── Session CRUD endpoints ────────────────────────────────────

/// List all sessions (summaries only, sorted by updated_at desc).
pub async fn sessions_list(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<stupid_agent::session::SessionSummary>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.read().await;
    store.list().map(Json).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to list sessions: {}", e),
            }),
        )
    })
}

#[derive(Deserialize)]
pub struct SessionCreateRequest {
    pub name: Option<String>,
}

/// Create a new empty session.
pub async fn sessions_create(
    State(state): State<Arc<AppState>>,
    Json(req): Json<SessionCreateRequest>,
) -> Result<(axum::http::StatusCode, Json<stupid_agent::session::Session>), (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    store
        .create(req.name.as_deref())
        .map(|s| (axum::http::StatusCode::CREATED, Json(s)))
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to create session: {}", e),
                }),
            )
        })
}

/// Get a full session with all messages.
pub async fn sessions_get(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<Json<stupid_agent::session::Session>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.read().await;
    match store.get(&id) {
        Ok(Some(session)) => Ok(Json(session)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to get session: {}", e),
            }),
        )),
    }
}

#[derive(Deserialize)]
pub struct SessionUpdateRequest {
    pub name: String,
}

/// Rename a session.
pub async fn sessions_update(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionUpdateRequest>,
) -> Result<Json<stupid_agent::session::Session>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    match store.rename(&id, &req.name) {
        Ok(Some(session)) => Ok(Json(session)),
        Ok(None) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to rename session: {}", e),
            }),
        )),
    }
}

/// Delete a session.
pub async fn sessions_delete(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
    let store = state.session_store.write().await;
    match store.delete(&id) {
        Ok(true) => Ok(axum::http::StatusCode::NO_CONTENT),
        Ok(false) => Err((
            axum::http::StatusCode::NOT_FOUND,
            Json(QueryErrorResponse {
                error: format!("Session not found: {}", id),
            }),
        )),
        Err(e) => Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            Json(QueryErrorResponse {
                error: format!("Failed to delete session: {}", e),
            }),
        )),
    }
}

// ── Session Execute endpoints ─────────────────────────────────

#[derive(Deserialize)]
pub struct SessionExecuteAgentRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

fn default_max_history() -> usize {
    10
}

#[derive(Serialize)]
pub struct SessionExecuteResponse<T: Serialize> {
    pub session: stupid_agent::session::SessionSummary,
    pub response: T,
}

/// Execute an agent within a session, persisting both user message and response.
pub async fn sessions_execute_agent(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionExecuteAgentRequest>,
) -> Result<Json<SessionExecuteResponse<stupid_agent::AgentResponse>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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

    // Execute with history
    let result = executor
        .execute_with_history(&req.agent_name, &req.task, &history, context, req.max_history)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

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

#[derive(Deserialize)]
#[allow(dead_code)]
pub struct SessionExecuteTeamRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    pub strategy: stupid_agent::TeamStrategy,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

/// Execute a team within a session, persisting both user message and team response.
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

#[derive(Deserialize)]
pub struct SessionExecuteRequest {
    pub task: String,
    #[serde(default)]
    pub context: serde_json::Value,
    #[serde(default = "default_max_history")]
    pub max_history: usize,
}

/// Execute directly against the LLM within a session (no agent selection needed).
pub async fn sessions_execute(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionExecuteRequest>,
) -> Result<Json<SessionExecuteResponse<stupid_agent::AgentResponse>>, (axum::http::StatusCode, Json<QueryErrorResponse>)> {
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

    // Execute directly (no agent routing)
    let result = executor
        .execute_direct(&req.task, &history, context, req.max_history)
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::BAD_REQUEST,
                Json(QueryErrorResponse {
                    error: e.to_string(),
                }),
            )
        })?;

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

// ── Session Streaming endpoint ──────────────────────────────────

#[derive(Deserialize)]
pub struct SessionStreamRequest {
    pub task: String,
    pub system_prompt: Option<String>,
    #[serde(default = "default_max_iterations")]
    pub max_iterations: usize,
}

fn default_max_iterations() -> usize {
    10
}

/// Stream an agentic loop response as SSE events within a session.
///
/// Uses the `AgenticLoop` from AppState, which wraps the LLM provider with
/// tool-use support. Each `StreamEvent` is sent as a JSON SSE data line.
/// After the stream completes, the assistant's response is persisted to the session.
pub async fn sessions_stream(
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
    Json(req): Json<SessionStreamRequest>,
) -> Result<
    Sse<impl futures::Stream<Item = Result<Event, Infallible>>>,
    (axum::http::StatusCode, Json<QueryErrorResponse>),
> {
    let agentic_loop = state.agentic_loop.clone().ok_or_else(|| {
        (
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            Json(QueryErrorResponse {
                error: "Agentic loop not configured. Check LLM provider settings.".into(),
            }),
        )
    })?;

    // Append user message to session
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

    // Load session history and convert to Conversation
    let session_messages = {
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

    let mut conversation = stupid_tool_runtime::Conversation::new(8192);

    // Load system prompt: prefer explicit request prompt, fall back to
    // auto-discovered project context (CLAUDE.md + skills + rules + agents).
    let system_prompt = if let Some(ref prompt) = req.system_prompt {
        prompt.clone()
    } else {
        let project_root = state.data_dir
            .parent()
            .unwrap_or(&state.data_dir)
            .join("agents/stupid-db-claude-code");
        stupid_tool_runtime::load_project_context(&project_root)
    };
    if !system_prompt.is_empty() {
        conversation = conversation.with_system_prompt(system_prompt);
    }

    // Convert SessionMessages to ConversationMessages (skip the last one — it's the
    // user message we just appended, which run_streaming will add itself)
    let history_messages = if session_messages.len() > 1 {
        &session_messages[..session_messages.len() - 1]
    } else {
        &[]
    };

    for msg in history_messages {
        match msg.role {
            stupid_agent::session::SessionMessageRole::User => {
                conversation.add_user_message(msg.content.clone());
            }
            stupid_agent::session::SessionMessageRole::Agent => {
                conversation.add_assistant_response(AssistantContent {
                    text: Some(msg.content.clone()),
                    tool_calls: Vec::new(),
                });
            }
            _ => {} // Skip Team/Error messages for the agentic loop
        }
    }

    // Set up streaming channel
    let (tx, rx) = tokio::sync::mpsc::channel::<StreamEvent>(256);

    // Use agents/stupid-db-claude-code as the working directory.
    // data_dir is typically `data/`; its parent is the project root.
    let agents_dir = state.data_dir
        .parent()
        .unwrap_or(&state.data_dir)
        .join("agents/stupid-db-claude-code");
    std::fs::create_dir_all(&agents_dir).ok();
    let tool_context = ToolContext {
        working_directory: agents_dir,
    };

    // Clone what we need for the background task
    let task = req.task.clone();
    let max_iterations = req.max_iterations;
    let session_store = state.session_store.clone();
    let session_id = id.clone();

    // Spawn the agentic loop in a background task
    tokio::spawn(async move {
        let loop_with_config = agentic_loop.with_max_iterations(max_iterations);

        let result = loop_with_config
            .run_streaming(&mut conversation, task, &tool_context, tx)
            .await;

        // Collect the assistant's response text from conversation history
        let response_text = conversation.messages().iter().rev().find_map(|msg| {
            if let ConversationMessage::Assistant(content) = msg {
                content.text.clone()
            } else {
                None
            }
        }).unwrap_or_default();

        // Persist assistant response to session
        let agent_msg = stupid_agent::session::SessionMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: stupid_agent::session::SessionMessageRole::Agent,
            content: response_text,
            timestamp: chrono::Utc::now(),
            agent_name: Some("assistant".to_string()),
            status: Some(if result.is_ok() { "completed" } else { "error" }.to_string()),
            execution_time_ms: None,
            team_outputs: None,
            agents_used: None,
            strategy: None,
        };

        let store = session_store.write().await;
        if let Err(e) = store.append_message(&session_id, agent_msg) {
            tracing::warn!(error = %e, session = %session_id, "Failed to persist assistant response");
        }

        if let Err(e) = result {
            tracing::warn!(error = %e, session = %session_id, "Agentic loop error");
        }
    });

    // Stream events as SSE
    use tokio_stream::StreamExt;
    let sse_stream = ReceiverStream::new(rx).map(|event| {
        let data = serde_json::to_string(&event).unwrap_or_else(|_| "{}".to_string());
        Ok(Event::default().data(data))
    });

    Ok(Sse::new(sse_stream))
}
