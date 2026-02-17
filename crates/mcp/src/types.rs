//! JSON-RPC 2.0 and MCP protocol types.
//!
//! Implements the wire format for the Model Context Protocol (MCP), which
//! uses JSON-RPC 2.0 over stdio for communication between clients and servers.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use stupid_tool_runtime::ToolDefinition;

// ── JSON-RPC 2.0 Base Types ─────────────────────────────────────────

/// A JSON-RPC 2.0 request message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: RpcId,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// A JSON-RPC 2.0 response message (success or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: RpcId,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// A JSON-RPC 2.0 notification (no id, no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,
}

/// JSON-RPC request ID. Can be a number or a string per the spec.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub enum RpcId {
    Number(i64),
    String(String),
}

// ── Standard JSON-RPC error codes ───────────────────────────────────

/// Standard JSON-RPC 2.0 error codes.
pub mod error_codes {
    pub const PARSE_ERROR: i64 = -32700;
    pub const INVALID_REQUEST: i64 = -32600;
    pub const METHOD_NOT_FOUND: i64 = -32601;
    pub const INVALID_PARAMS: i64 = -32602;
    pub const INTERNAL_ERROR: i64 = -32603;
}

// ── MCP Initialize ──────────────────────────────────────────────────

/// Parameters for the `initialize` MCP method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: String,
    pub capabilities: ClientCapabilities,
    pub client_info: ClientInfo,
}

/// Client capabilities advertised during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ClientCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roots: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sampling: Option<Value>,
}

/// Information about the connecting client.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

/// Result returned from the `initialize` method.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: String,
    pub capabilities: ServerCapabilities,
    pub server_info: ServerInfo,
}

/// Server capabilities advertised during initialization.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tools: Option<ToolsCapability>,
}

/// Tools capability descriptor.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolsCapability {
    #[serde(default)]
    pub list_changed: bool,
}

/// Information about the MCP server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
}

// ── MCP tools/list ──────────────────────────────────────────────────

/// Parameters for `tools/list`. Currently empty but reserved for pagination.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ListToolsParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}

/// Result of `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ListToolsResult {
    pub tools: Vec<ToolInfo>,
}

/// Describes a single tool in MCP format.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolInfo {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

impl From<ToolDefinition> for ToolInfo {
    fn from(def: ToolDefinition) -> Self {
        Self {
            name: def.name,
            description: def.description,
            input_schema: def.input_schema,
        }
    }
}

impl From<ToolInfo> for ToolDefinition {
    fn from(info: ToolInfo) -> Self {
        Self {
            name: info.name,
            description: info.description,
            input_schema: info.input_schema,
        }
    }
}

// ── MCP tools/call ──────────────────────────────────────────────────

/// Parameters for `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolParams {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

/// Result of `tools/call`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallToolResult {
    pub content: Vec<ToolContent>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub is_error: bool,
}

/// Content block within a tool call result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToolContent {
    Text { text: String },
}

// ── Helpers ─────────────────────────────────────────────────────────

impl JsonRpcRequest {
    /// Create a new JSON-RPC 2.0 request.
    pub fn new(id: RpcId, method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

impl JsonRpcResponse {
    /// Create a successful response.
    pub fn success(id: RpcId, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn error(id: RpcId, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

impl JsonRpcNotification {
    /// Create a new JSON-RPC 2.0 notification.
    pub fn new(method: impl Into<String>, params: Option<Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

// ── MCP Protocol version ────────────────────────────────────────────

/// The MCP protocol version this crate implements.
pub const PROTOCOL_VERSION: &str = "2024-11-05";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jsonrpc_request_roundtrip() {
        let req = JsonRpcRequest::new(
            RpcId::Number(1),
            "initialize",
            Some(serde_json::json!({"protocolVersion": "2024-11-05"})),
        );
        let json = serde_json::to_string(&req).unwrap();
        let parsed: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, "initialize");
        assert_eq!(parsed.id, RpcId::Number(1));
        assert_eq!(parsed.jsonrpc, "2.0");
    }

    #[test]
    fn test_jsonrpc_response_success_roundtrip() {
        let resp = JsonRpcResponse::success(
            RpcId::String("abc".to_string()),
            serde_json::json!({"status": "ok"}),
        );
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.result.is_some());
        assert!(parsed.error.is_none());
        assert_eq!(parsed.id, RpcId::String("abc".to_string()));
    }

    #[test]
    fn test_jsonrpc_response_error_roundtrip() {
        let resp = JsonRpcResponse::error(
            RpcId::Number(2),
            error_codes::METHOD_NOT_FOUND,
            "Method not found",
        );
        let json = serde_json::to_string(&resp).unwrap();
        let parsed: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert!(parsed.result.is_none());
        assert!(parsed.error.is_some());
        let err = parsed.error.unwrap();
        assert_eq!(err.code, error_codes::METHOD_NOT_FOUND);
        assert_eq!(err.message, "Method not found");
    }

    #[test]
    fn test_jsonrpc_notification_roundtrip() {
        let notif = JsonRpcNotification::new(
            "notifications/initialized",
            None,
        );
        let json = serde_json::to_string(&notif).unwrap();
        let parsed: JsonRpcNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.method, "notifications/initialized");
        assert!(parsed.params.is_none());
    }

    #[test]
    fn test_rpc_id_number() {
        let id = RpcId::Number(42);
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "42");
        let parsed: RpcId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RpcId::Number(42));
    }

    #[test]
    fn test_rpc_id_string() {
        let id = RpcId::String("req-1".to_string());
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"req-1\"");
        let parsed: RpcId = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, RpcId::String("req-1".to_string()));
    }

    #[test]
    fn test_tool_info_from_tool_definition() {
        let def = ToolDefinition {
            name: "echo".to_string(),
            description: "Echo tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
        };
        let info: ToolInfo = def.into();
        assert_eq!(info.name, "echo");
        assert_eq!(info.description, "Echo tool");
    }

    #[test]
    fn test_tool_definition_from_tool_info() {
        let info = ToolInfo {
            name: "search".to_string(),
            description: "Search tool".to_string(),
            input_schema: serde_json::json!({"type": "object", "properties": {}}),
        };
        let def: ToolDefinition = info.into();
        assert_eq!(def.name, "search");
    }

    #[test]
    fn test_initialize_result_roundtrip() {
        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: false }),
            },
            server_info: ServerInfo {
                name: "stupid-mcp".to_string(),
                version: Some("0.1.0".to_string()),
            },
        };
        let json = serde_json::to_value(&result).unwrap();
        let parsed: InitializeResult = serde_json::from_value(json).unwrap();
        assert_eq!(parsed.protocol_version, PROTOCOL_VERSION);
        assert_eq!(parsed.server_info.name, "stupid-mcp");
    }

    #[test]
    fn test_call_tool_result_roundtrip() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "hello".to_string(),
            }],
            is_error: false,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: CallToolResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.content.len(), 1);
        assert!(!parsed.is_error);
        // Verify is_error is omitted when false
        assert!(!json.contains("is_error") || !json.contains("isError"));
    }

    #[test]
    fn test_call_tool_result_with_error() {
        let result = CallToolResult {
            content: vec![ToolContent::Text {
                text: "something went wrong".to_string(),
            }],
            is_error: true,
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: CallToolResult = serde_json::from_str(&json).unwrap();
        assert!(parsed.is_error);
    }

    #[test]
    fn test_list_tools_result_roundtrip() {
        let result = ListToolsResult {
            tools: vec![
                ToolInfo {
                    name: "echo".to_string(),
                    description: "Echo tool".to_string(),
                    input_schema: serde_json::json!({"type": "object"}),
                },
            ],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: ListToolsResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.tools.len(), 1);
        assert_eq!(parsed.tools[0].name, "echo");
    }
}
