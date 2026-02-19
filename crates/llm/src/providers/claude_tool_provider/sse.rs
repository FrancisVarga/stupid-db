//! SSE event parsing and content-block state tracking for the Claude streaming API.

use serde_json::Value;
use tracing::trace;

use stupid_tool_runtime::stream::{StopReason, StreamEvent};

/// Parse a single SSE event (type + data) into zero or more [`StreamEvent`]s.
pub(super) fn parse_sse_event(event_type: &str, data: &str) -> Vec<StreamEvent> {
    let mut events = Vec::new();

    match event_type {
        "content_block_start" => {
            if let Ok(parsed) = serde_json::from_str::<Value>(data) {
                let block = &parsed["content_block"];
                match block["type"].as_str() {
                    Some("text") => {
                        // Text block starting -- initial text if present
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
                // Emit ToolCallEnd only for tool_use blocks -- resolved in the
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
            // message_stop is just a sentinel -- nothing to emit.
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
pub(super) struct BlockTracker {
    /// Maps content-block index to (block_type, tool_use_id).
    blocks: Vec<(String, String)>,
}

impl BlockTracker {
    pub(super) fn new() -> Self {
        Self { blocks: Vec::new() }
    }

    pub(super) fn register_block(&mut self, index: usize, block_type: &str, id: &str) {
        if index >= self.blocks.len() {
            self.blocks
                .resize(index + 1, (String::new(), String::new()));
        }
        self.blocks[index] = (block_type.to_string(), id.to_string());
    }

    pub(super) fn resolve(&self, placeholder_id: &str) -> Option<(String, String)> {
        if let Some(idx_str) = placeholder_id.strip_prefix("__index_") {
            if let Ok(idx) = idx_str.parse::<usize>() {
                return self.blocks.get(idx).cloned();
            }
        }
        None
    }
}
