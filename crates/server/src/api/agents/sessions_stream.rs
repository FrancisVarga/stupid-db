//! Session streaming endpoint: agentic loop with tool-use over SSE.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::response::sse::{Event, Sse};
use axum::Json;
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;

use stupid_tool_runtime::conversation::{AssistantContent, ConversationMessage};
use stupid_tool_runtime::stream::StreamEvent;
use stupid_tool_runtime::tool::ToolContext;

use crate::state::AppState;

use super::super::QueryErrorResponse;
use super::types::SessionStreamRequest;

/// Stream an agentic loop response within a session
///
/// Uses the AgenticLoop from AppState with tool-use support. Each StreamEvent
/// is sent as a JSON SSE data line. After the stream completes, the assistant's
/// response is persisted to the session. Event types: text_delta, tool_call,
/// tool_result, error, done.
#[utoipa::path(
    post,
    path = "/sessions/{id}/stream",
    tag = "Sessions",
    params(
        ("id" = String, Path, description = "Session ID")
    ),
    request_body = SessionStreamRequest,
    responses(
        (status = 200, description = "SSE stream of agentic loop events", content_type = "text/event-stream"),
        (status = 404, description = "Session not found", body = QueryErrorResponse),
        (status = 503, description = "Agentic loop not configured", body = QueryErrorResponse)
    )
)]
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

    // Ensure session exists (auto-create if new), then append user message
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
        // Auto-create session if it doesn't exist yet (e.g. rule-builder chat)
        store.get_or_create(&id).map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                Json(QueryErrorResponse {
                    error: format!("Failed to ensure session: {}", e),
                }),
            )
        })?;
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

    // Convert SessionMessages to ConversationMessages (skip the last one -- it's the
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
