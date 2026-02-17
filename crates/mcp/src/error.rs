//! Error types for the MCP crate.

use crate::types::{error_codes, JsonRpcError};

/// Errors that can occur during MCP operations.
#[derive(Debug, thiserror::Error)]
pub enum McpError {
    /// Failed to parse JSON.
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Transport I/O error.
    #[error("Transport error: {0}")]
    Transport(#[from] std::io::Error),

    /// The requested method is not supported.
    #[error("Method not found: {0}")]
    MethodNotFound(String),

    /// Invalid parameters for a method.
    #[error("Invalid params: {0}")]
    InvalidParams(String),

    /// The requested tool was not found in the registry.
    #[error("Tool not found: {0}")]
    ToolNotFound(String),

    /// Tool execution failed.
    #[error("Tool execution failed: {0}")]
    ToolExecution(String),

    /// Protocol version mismatch.
    #[error("Unsupported protocol version: {0}")]
    UnsupportedVersion(String),

    /// Server/client not initialized.
    #[error("Not initialized: call initialize first")]
    NotInitialized,

    /// The MCP server process exited or is unavailable.
    #[error("Server unavailable: {0}")]
    ServerUnavailable(String),
}

impl McpError {
    /// Convert to a JSON-RPC error object.
    pub fn to_rpc_error(&self) -> JsonRpcError {
        let (code, message) = match self {
            McpError::JsonParse(_) => (error_codes::PARSE_ERROR, self.to_string()),
            McpError::MethodNotFound(_) => (error_codes::METHOD_NOT_FOUND, self.to_string()),
            McpError::InvalidParams(_) => (error_codes::INVALID_PARAMS, self.to_string()),
            McpError::ToolNotFound(_) => (error_codes::INVALID_PARAMS, self.to_string()),
            _ => (error_codes::INTERNAL_ERROR, self.to_string()),
        };
        JsonRpcError {
            code,
            message,
            data: None,
        }
    }
}
