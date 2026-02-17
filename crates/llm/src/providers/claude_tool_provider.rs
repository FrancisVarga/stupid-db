//! Claude (Anthropic API) implementation of [`ToolAwareLlmProvider`].
//!
//! Supports streaming tool use via SSE, translating between the Anthropic Messages
//! API format and the provider-agnostic [`StreamEvent`] / [`ConversationMessage`] types.

use async_trait::async_trait;
use futures::stream::{self, Stream};
use serde_json::{json, Value};
use std::pin::Pin;
use tracing::{debug, trace};

use stupid_tool_runtime::{
    conversation::ConversationMessage,
    provider::{LlmError, ToolAwareLlmProvider},
    stream::{StopReason, StreamEvent},
    tool::ToolDefinition,
};

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

// ---------------------------------------------------------------------------
// Message translation
// ---------------------------------------------------------------------------

/// Translate a [`ToolDefinition`] into the Claude API tool format.
fn tool_definition_to_claude(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

/// Translate a [`ConversationMessage`] into a Claude API message object.
fn message_to_claude(msg: &ConversationMessage) -> Value {
    match msg {
        ConversationMessage::User(text) => json!({
            "role": "user",
            "content": text,
        }),
        ConversationMessage::Assistant(content) => {
            let mut blocks: Vec<Value> = Vec::new();
            if let Some(text) = &content.text {
                blocks.push(json!({"type": "text", "text": text}));
            }
            for tc in &content.tool_calls {
                blocks.push(json!({
                    "type": "tool_use",
                    "id": tc.id,
                    "name": tc.name,
                    "input": tc.input,
                }));
            }
            json!({
                "role": "assistant",
                "content": blocks,
            })
        }
        ConversationMessage::ToolResult(result) => json!({
            "role": "user",
            "content": [{
                "type": "tool_result",
                "tool_use_id": result.tool_call_id,
                "content": result.content,
                "is_error": result.is_error,
            }],
        }),
    }
}

// ---------------------------------------------------------------------------
// SSE parsing
// ---------------------------------------------------------------------------

/// Parse a single SSE event (type + data) into zero or more [`StreamEvent`]s.
fn parse_sse_event(event_type: &str, data: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    match event_type {
        "content_block_start" => {
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                let block = &parsed["content_block"];
                match block["type"].as_str() {
                    Some("text") => {
                        // Text block starting — initial text if present
                        if let Some(text) = block["text"].as_str() {
                            if !text.is_empty() {
                                events.push(StreamEvent::TextDelta {
                                    text: text.to_string(),
                                });
                            }
                        }
                    }
                    Some("tool_use") => {
                        let id = block["id"].as_str().unwrap_or("").to_string();
                        let name = block["name"].as_str().unwrap_or("").to_string();
                        events.push(StreamEvent::ToolCallStart { id, name });
                    }
                    _ => {}
                }
            }
        }
        "content_block_delta" => {
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                let delta = &parsed["delta"];
                match delta["type"].as_str() {
                    Some("text_delta") => {
                        if let Some(text) = delta["text"].as_str() {
                            events.push(StreamEvent::TextDelta {
                                text: text.to_string(),
                            });
                        }
                    }
                    Some("input_json_delta") => {
                        if let Some(json_str) = delta["partial_json"].as_str() {
                            // We need the content_block index to find the tool call id.
                            // The index is in the outer object, but we need the id from
                            // the content_block_start.  The caller tracks state; here we
                            // use the index as a stand-in that the stream wrapper resolves.
                            let index = parsed["index"].as_u64().unwrap_or(0);
                            events.push(StreamEvent::ToolCallDelta {
                                id: format!("__index_{}", index),
                                arguments_delta: json_str.to_string(),
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        "content_block_stop" => {
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                let index = parsed["index"].as_u64().unwrap_or(0);
                // Emit ToolCallEnd only for tool_use blocks — resolved in the
                // stateful stream wrapper below.
                events.push(StreamEvent::ToolCallEnd {
                    id: format!("__index_{}", index),
                });
            }
        }
        "message_delta" => {
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                let stop_reason = match parsed["delta"]["stop_reason"].as_str() {
                    Some("end_turn") => StopReason::EndTurn,
                    Some("tool_use") => StopReason::ToolUse,
                    Some("max_tokens") => StopReason::MaxTokens,
                    Some("stop_sequence") => StopReason::StopSequence,
                    _ => StopReason::EndTurn,
                };
                events.push(StreamEvent::MessageEnd { stop_reason });
            }
        }
        "message_stop" => {
            // message_delta already emitted MessageEnd with stop_reason.
            // message_stop is just a sentinel — nothing to emit.
        }
        "message_start" | "ping" => {
            // Informational, no action needed.
        }
        "error" => {
            let message = serde_json::from_str::<Value>(data)
                .ok()
                .and_then(|v| v["error"]["message"].as_str().map(String::from))
                .unwrap_or_else(|| data.to_string());
            events.push(StreamEvent::Error { message });
        }
        _ => {
            trace!(event_type, "ignoring unknown SSE event type");
        }
    }

    events
}

/// Tracks per-block state so we can resolve `__index_N` placeholders to real
/// tool_use IDs and filter out spurious ToolCallEnd for text blocks.
struct BlockTracker {
    /// Maps content-block index to (block_type, tool_use_id).
    blocks: Vec<(String, String)>,
}

impl BlockTracker {
    fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    fn register_block(&mut self, index: usize, block_type: &str, id: &str) {
        if index >= self.blocks.len() {
            self.blocks
                .resize(index + 1, (String::new(), String::new()));
        }
        self.blocks[index] = (block_type.to_string(), id.to_string());
    }

    fn resolve(&self, placeholder_id: &str) -> Option<(String, String)> {
        if let Some(idx_str) = placeholder_id.strip_prefix("__index_") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                return self.blocks.get(idx).cloned();
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Trait implementation
// ---------------------------------------------------------------------------

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
                                                        // Text block stop — not a ToolCallEnd
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
                        // Stream ended — drain remaining pending
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

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use stupid_tool_runtime::conversation::AssistantContent;
    use stupid_tool_runtime::tool::ToolCall as TrToolCall;
    use stupid_tool_runtime::ToolResult as TrToolResult;

    #[test]
    fn test_tool_definition_translation() {
        let def = ToolDefinition {
            name: "bash_execute".to_string(),
            description: "Execute a bash command".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "command": { "type": "string", "description": "The command to run" }
                },
                "required": ["command"]
            }),
        };

        let claude_json = tool_definition_to_claude(&def);

        assert_eq!(claude_json["name"], "bash_execute");
        assert_eq!(claude_json["description"], "Execute a bash command");
        assert_eq!(claude_json["input_schema"]["type"], "object");
        assert_eq!(
            claude_json["input_schema"]["properties"]["command"]["type"],
            "string"
        );
    }

    #[test]
    fn test_user_message_translation() {
        let msg = ConversationMessage::User("Hello Claude".to_string());
        let claude_json = message_to_claude(&msg);

        assert_eq!(claude_json["role"], "user");
        assert_eq!(claude_json["content"], "Hello Claude");
    }

    #[test]
    fn test_assistant_text_only_translation() {
        let msg = ConversationMessage::Assistant(AssistantContent {
            text: Some("I can help with that.".to_string()),
            tool_calls: vec![],
        });
        let claude_json = message_to_claude(&msg);

        assert_eq!(claude_json["role"], "assistant");
        let content = claude_json["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[0]["text"], "I can help with that.");
    }

    #[test]
    fn test_assistant_mixed_content_translation() {
        let msg = ConversationMessage::Assistant(AssistantContent {
            text: Some("Let me check that.".to_string()),
            tool_calls: vec![TrToolCall {
                id: "toolu_01".to_string(),
                name: "bash_execute".to_string(),
                input: json!({"command": "ls -la"}),
            }],
        });
        let claude_json = message_to_claude(&msg);

        let content = claude_json["content"].as_array().unwrap();
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["type"], "text");
        assert_eq!(content[1]["type"], "tool_use");
        assert_eq!(content[1]["id"], "toolu_01");
        assert_eq!(content[1]["name"], "bash_execute");
        assert_eq!(content[1]["input"]["command"], "ls -la");
    }

    #[test]
    fn test_tool_result_translation() {
        let msg = ConversationMessage::ToolResult(TrToolResult {
            tool_call_id: "toolu_01".to_string(),
            content: "file1.txt\nfile2.txt".to_string(),
            is_error: false,
        });
        let claude_json = message_to_claude(&msg);

        assert_eq!(claude_json["role"], "user");
        let content = claude_json["content"].as_array().unwrap();
        assert_eq!(content[0]["type"], "tool_result");
        assert_eq!(content[0]["tool_use_id"], "toolu_01");
        assert_eq!(content[0]["content"], "file1.txt\nfile2.txt");
        assert!(!content[0]["is_error"].as_bool().unwrap());
    }

    #[test]
    fn test_sse_text_delta() {
        let events = parse_sse_event(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::TextDelta { text } => assert_eq!(text, "Hello"),
            other => panic!("expected TextDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_tool_call_start() {
        let events = parse_sse_event(
            "content_block_start",
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_abc","name":"bash_execute"}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::ToolCallStart { id, name } => {
                assert_eq!(id, "toolu_abc");
                assert_eq!(name, "bash_execute");
            }
            other => panic!("expected ToolCallStart, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_tool_call_delta() {
        let events = parse_sse_event(
            "content_block_delta",
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"command\":"}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::ToolCallDelta {
                id,
                arguments_delta,
            } => {
                assert_eq!(id, "__index_1");
                assert_eq!(arguments_delta, "{\"command\":");
            }
            other => panic!("expected ToolCallDelta, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_message_delta_end_turn() {
        let events = parse_sse_event(
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::MessageEnd { stop_reason } => {
                assert_eq!(*stop_reason, StopReason::EndTurn);
            }
            other => panic!("expected MessageEnd, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_message_delta_tool_use() {
        let events = parse_sse_event(
            "message_delta",
            r#"{"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":10}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::MessageEnd { stop_reason } => {
                assert_eq!(*stop_reason, StopReason::ToolUse);
            }
            other => panic!("expected MessageEnd, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_error_event() {
        let events = parse_sse_event(
            "error",
            r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#,
        );
        assert_eq!(events.len(), 1);
        match &events[0] {
            StreamEvent::Error { message } => assert_eq!(message, "Overloaded"),
            other => panic!("expected Error, got {:?}", other),
        }
    }

    #[test]
    fn test_sse_ping_ignored() {
        let events = parse_sse_event("ping", "{}");
        assert!(events.is_empty());
    }

    #[test]
    fn test_sse_message_stop_ignored() {
        let events = parse_sse_event("message_stop", r#"{"type":"message_stop"}"#);
        assert!(events.is_empty());
    }

    #[test]
    fn test_block_tracker_resolution() {
        let mut tracker = BlockTracker::new();
        tracker.register_block(0, "text", "");
        tracker.register_block(1, "tool_use", "toolu_abc");

        let resolved = tracker.resolve("__index_1").unwrap();
        assert_eq!(resolved.0, "tool_use");
        assert_eq!(resolved.1, "toolu_abc");

        let resolved_text = tracker.resolve("__index_0").unwrap();
        assert_eq!(resolved_text.0, "text");

        assert!(tracker.resolve("__index_99").is_none());
        assert!(tracker.resolve("not_a_placeholder").is_none());
    }
}
