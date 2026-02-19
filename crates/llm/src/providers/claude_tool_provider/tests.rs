//! Unit tests for the Claude tool provider.

use serde_json::json;

use stupid_tool_runtime::conversation::{AssistantContent, ConversationMessage};
use stupid_tool_runtime::stream::{StopReason, StreamEvent};
use stupid_tool_runtime::tool::{ToolCall as TrToolCall, ToolDefinition};
use stupid_tool_runtime::ToolResult as TrToolResult;

use super::sse::{parse_sse_event, BlockTracker};
use super::translate::{message_to_claude, tool_definition_to_claude};

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
