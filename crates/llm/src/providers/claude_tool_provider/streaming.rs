//! [`ToolAwareLlmProvider`] trait implementation for the Claude streaming API.

use async_trait::async_trait;
use futures::stream::{self, Stream};
use serde_json::{json, Value};
use std::pin::Pin;
use tracing::debug;

use stupid_tool_runtime::{
    conversation::ConversationMessage,
    provider::{LlmError, ToolAwareLlmProvider},
    stream::StreamEvent,
    tool::ToolDefinition,
};

use super::sse::{parse_sse_event, BlockTracker};
use super::translate::{message_to_claude, tool_definition_to_claude};

/// Claude (Anthropic) provider with streaming tool-use support.
///
/// Uses the Anthropic Messages API (`/v1/messages`) with `stream: true` to emit
/// incremental [`StreamEvent`]s that the agentic loop can consume.
pub struct ClaudeToolProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
    base_url: String,
}

impl ClaudeToolProvider {
    /// Create a new Claude tool provider.
    ///
    /// # Arguments
    /// * `api_key` - Anthropic API key
    /// * `model` - Model name (e.g. `"claude-sonnet-4-20250514"`)
    /// * `base_url` - API base URL (e.g. `"https://api.anthropic.com"`)
    pub fn new(api_key: String, model: String, base_url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            api_key,
            model,
            base_url,
        }
    }

    /// Create a provider with sensible defaults.
    pub fn with_defaults(api_key: String) -> Self {
        Self::new(
            api_key,
            "claude-sonnet-4-20250514".to_string(),
            "https://api.anthropic.com".to_string(),
        )
    }
}

#[async_trait]
impl ToolAwareLlmProvider for ClaudeToolProvider {
    async fn stream_with_tools(
        &self,
        messages: Vec<ConversationMessage>,
        system_prompt: Option<String>,
        tools: Vec<ToolDefinition>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError> {
        let url = format!("{}/v1/messages", self.base_url);

        let api_messages: Vec<Value> = messages.iter().map(message_to_claude).collect();
        let api_tools: Vec<Value> = tools.iter().map(tool_definition_to_claude).collect();

        let mut body = json!({
            "model": self.model,
            "messages": api_messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": true,
        });

        if !api_tools.is_empty() {
            body["tools"] = json!(api_tools);
        }

        if let Some(system) = &system_prompt {
            body["system"] = json!(system);
        }

        debug!(model = %self.model, url = %url, "starting Claude streaming request");

        let response = self
            .client
            .post(&url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| LlmError::NetworkError(e.to_string()))?;

        let status = response.status().as_u16();

        // Handle non-200 responses
        if status != 200 {
            let body_text = response.text().await.unwrap_or_default();

            if status == 401 {
                return Err(LlmError::AuthError);
            }
            if status == 429 {
                // Try to parse retry-after from the response
                let retry_after = serde_json::from_str::<Value>(&body_text)
                    .ok()
                    .and_then(|v| v["error"]["retry_after_secs"].as_u64())
                    .unwrap_or(30);
                return Err(LlmError::RateLimited {
                    retry_after_secs: retry_after,
                });
            }
            return Err(LlmError::ApiError {
                status,
                message: body_text,
            });
        }

        // Stream the SSE response, parsing events and tracking block state.
        let byte_stream = response.bytes_stream();

        type ByteStream =
            Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>;

        struct State {
            bytes: ByteStream,
            buffer: String,
            tracker: BlockTracker,
            pending: Vec<StreamEvent>,
        }

        let state = State {
            bytes: Box::pin(byte_stream),
            buffer: String::new(),
            tracker: BlockTracker::new(),
            pending: Vec::new(),
        };

        let event_stream = stream::unfold(state, move |mut state| async move {
            use futures::StreamExt;
            loop {
                // First, drain any pending events
                if let Some(evt) = state.pending.pop() {
                    return Some((Ok(evt), state));
                }

                // Read more bytes from the response
                match state.bytes.next().await {
                    Some(Ok(chunk)) => {
                        let text = String::from_utf8_lossy(&chunk);
                        state.buffer.push_str(&text);

                        // Process complete lines from the buffer
                        while let Some(newline_pos) = state.buffer.find('\n') {
                            let line = state.buffer[..newline_pos]
                                .trim_end_matches('\r')
                                .to_string();
                            state.buffer = state.buffer[newline_pos + 1..].to_string();

                            // SSE parsing: lines are "event: X", "data: Y", or empty
                            if line.is_empty() {
                                continue;
                            }

                            if let Some(event_type) = line.strip_prefix("event: ") {
                                let event_type = event_type.to_string();
                                // Peek at the next line for the data
                                if let Some(data_newline) = state.buffer.find('\n') {
                                    let data_line = state.buffer[..data_newline]
                                        .trim_end_matches('\r')
                                        .to_string();
                                    state.buffer =
                                        state.buffer[data_newline + 1..].to_string();

                                    let data = data_line
                                        .strip_prefix("data: ")
                                        .unwrap_or(&data_line);

                                    let mut raw_events =
                                        parse_sse_event(&event_type, data);

                                    // Post-process: register blocks and resolve IDs
                                    for evt in &mut raw_events {
                                        match evt {
                                            StreamEvent::ToolCallStart {
                                                id,
                                                name: _,
                                            } => {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<Value>(data)
                                                {
                                                    let idx = parsed["index"]
                                                        .as_u64()
                                                        .unwrap_or(0)
                                                        as usize;
                                                    state.tracker.register_block(
                                                        idx, "tool_use", id,
                                                    );
                                                }
                                            }
                                            StreamEvent::TextDelta { .. } => {
                                                if let Ok(parsed) =
                                                    serde_json::from_str::<Value>(data)
                                                {
                                                    if let Some(idx) =
                                                        parsed["index"].as_u64()
                                                    {
                                                        state.tracker.register_block(
                                                            idx as usize,
                                                            "text",
                                                            "",
                                                        );
                                                    }
                                                }
                                            }
                                            StreamEvent::ToolCallDelta {
                                                id,
                                                arguments_delta: _,
                                            } => {
                                                if let Some((_, real_id)) =
                                                    state.tracker.resolve(id)
                                                {
                                                    *id = real_id;
                                                }
                                            }
                                            StreamEvent::ToolCallEnd { id } => {
                                                if let Some((block_type, real_id)) =
                                                    state.tracker.resolve(id)
                                                {
                                                    if block_type == "tool_use" {
                                                        *id = real_id;
                                                    } else {
                                                        // Text block stop -- not a ToolCallEnd
                                                        *evt = StreamEvent::TextDelta {
                                                            text: String::new(),
                                                        };
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                    }

                                    // Filter out empty text deltas used as sentinels
                                    raw_events.retain(|e| {
                                        !matches!(e, StreamEvent::TextDelta { text } if text.is_empty())
                                    });

                                    state.pending.extend(raw_events);
                                }
                            }
                        }
                    }
                    Some(Err(e)) => {
                        return Some((
                            Err(LlmError::StreamError(e.to_string())),
                            state,
                        ));
                    }
                    None => {
                        // Stream ended -- drain remaining pending
                        if let Some(evt) = state.pending.pop() {
                            return Some((Ok(evt), state));
                        }
                        return None;
                    }
                }
            }
        });

        Ok(Box::pin(event_stream))
    }

    fn provider_name(&self) -> &str {
        "claude"
    }
}
