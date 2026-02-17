use serde::{Deserialize, Serialize};

/// Events emitted during streaming LLM responses.
/// Provider-agnostic — translated from Claude/OpenAI formats in the provider layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StreamEvent {
    /// A chunk of text from the assistant
    TextDelta {
        text: String,
    },
    /// Start of a tool call (LLM wants to execute a tool)
    ToolCallStart {
        id: String,
        name: String,
    },
    /// Incremental JSON argument data for a tool call
    ToolCallDelta {
        id: String,
        arguments_delta: String,
    },
    /// Tool call arguments are complete
    ToolCallEnd {
        id: String,
    },
    /// The entire message is complete
    MessageEnd {
        stop_reason: StopReason,
    },
    /// An error occurred during streaming
    Error {
        message: String,
    },
    /// A tool is about to be executed
    ToolExecutionStart {
        id: String,
        name: String,
    },
    /// A tool has finished executing
    ToolExecutionResult {
        id: String,
        content: String,
        is_error: bool,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StopReason {
    /// Normal end of response
    EndTurn,
    /// Model wants to use tools
    ToolUse,
    /// Hit max tokens limit
    MaxTokens,
    /// Stopped by stop sequence
    StopSequence,
}
