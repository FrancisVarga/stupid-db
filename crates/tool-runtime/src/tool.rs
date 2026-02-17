use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fmt;

/// Describes a tool's interface for LLM consumption.
/// Maps to Claude's tool format and OpenAI's function format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolDefinition {
    /// Unique tool name (e.g., "bash_execute", "file_read")
    pub name: String,
    /// Human-readable description for the LLM
    pub description: String,
    /// JSON Schema describing the expected input
    pub input_schema: Value,
}

/// Represents an LLM requesting execution of a tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    /// Unique ID for this invocation (used to match results)
    pub id: String,
    /// Tool name to execute
    pub name: String,
    /// JSON input arguments
    pub input: Value,
}

/// Result of executing a tool, sent back to the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    /// Must match the ToolCall id
    pub tool_call_id: String,
    /// Result content (text or structured)
    pub content: String,
    /// Whether this result represents an error
    pub is_error: bool,
}

/// Context passed to tool execution, providing access to permissions and state.
pub struct ToolContext {
    /// Working directory for file/bash operations
    pub working_directory: std::path::PathBuf,
}

/// The primary extension point: all tools implement this trait.
///
/// Tools are object-safe, Send + Sync, and async.
#[async_trait]
pub trait Tool: Send + Sync {
    /// Returns the tool's definition (name, description, JSON Schema).
    fn definition(&self) -> ToolDefinition;

    /// Execute the tool with the given JSON input.
    async fn execute(&self, input: Value, context: &ToolContext) -> Result<ToolResult, ToolError>;
}

#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Execution failed: {0}")]
    ExecutionFailed(String),
    #[error("Permission denied: {0}")]
    PermissionDenied(String),
    #[error("Timeout after {0:?}")]
    Timeout(std::time::Duration),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

impl fmt::Display for ToolDefinition {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}({})", self.name, self.description)
    }
}

/// Simple echo tool for testing purposes.
pub struct EchoTool;

#[async_trait]
impl Tool for EchoTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: "echo".to_string(),
            description: "Echoes back the input message. For testing.".to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "message": {
                        "type": "string",
                        "description": "The message to echo back"
                    }
                },
                "required": ["message"]
            }),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let message = input
            .get("message")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidInput("missing 'message' field".to_string()))?;

        Ok(ToolResult {
            tool_call_id: String::new(), // Set by caller
            content: message.to_string(),
            is_error: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_definition_serialization() {
        let def = ToolDefinition {
            name: "test_tool".to_string(),
            description: "A test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let json = serde_json::to_string(&def).unwrap();
        let roundtrip: ToolDefinition = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.name, "test_tool");
    }

    #[test]
    fn test_tool_call_serialization() {
        let call = ToolCall {
            id: "call_001".to_string(),
            name: "echo".to_string(),
            input: serde_json::json!({"message": "hello"}),
        };
        let json = serde_json::to_string(&call).unwrap();
        let roundtrip: ToolCall = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.id, "call_001");
        assert_eq!(roundtrip.name, "echo");
    }

    #[test]
    fn test_tool_result_serialization() {
        let result = ToolResult {
            tool_call_id: "call_001".to_string(),
            content: "hello".to_string(),
            is_error: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let roundtrip: ToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(roundtrip.tool_call_id, "call_001");
        assert!(!roundtrip.is_error);
    }

    #[tokio::test]
    async fn test_echo_tool() {
        let tool = EchoTool;
        let def = tool.definition();
        assert_eq!(def.name, "echo");

        let ctx = ToolContext {
            working_directory: std::path::PathBuf::from("/tmp"),
        };
        let result = tool
            .execute(serde_json::json!({"message": "hello world"}), &ctx)
            .await
            .unwrap();
        assert_eq!(result.content, "hello world");
        assert!(!result.is_error);
    }
}
