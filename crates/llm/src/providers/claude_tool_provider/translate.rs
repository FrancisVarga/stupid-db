//! Translation between provider-agnostic conversation types and the Claude API format.

use serde_json::{json, Value};

use stupid_tool_runtime::{conversation::ConversationMessage, tool::ToolDefinition};

/// Translate a [`ToolDefinition`] into the Claude API tool format.
pub(super) fn tool_definition_to_claude(tool: &ToolDefinition) -> Value {
    json!({
        "name": tool.name,
        "description": tool.description,
        "input_schema": tool.input_schema,
    })
}

/// Translate a [`ConversationMessage`] into a Claude API message object.
pub(super) fn message_to_claude(msg: &ConversationMessage) -> Value {
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
