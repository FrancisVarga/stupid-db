//! MCP server implementation.
//!
//! Wraps a `ToolRegistry` and exposes its tools over the MCP protocol.
//! Handles JSON-RPC requests and dispatches them to the appropriate handlers.

use serde_json::Value;
use std::path::PathBuf;

use stupid_tool_runtime::tool::ToolContext;
use stupid_tool_runtime::ToolRegistry;

use crate::error::McpError;
use crate::transport::McpTransport;
use crate::types::*;

/// MCP server that bridges a `ToolRegistry` to MCP clients.
pub struct McpServer {
    registry: ToolRegistry,
    server_name: String,
    server_version: String,
    initialized: bool,
    working_directory: PathBuf,
}

impl McpServer {
    /// Create a new MCP server wrapping the given tool registry.
    pub fn new(registry: ToolRegistry) -> Self {
        Self {
            registry,
            server_name: "stupid-mcp".to_string(),
            server_version: env!("CARGO_PKG_VERSION").to_string(),
            initialized: false,
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
        }
    }

    /// Set the server name.
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.server_name = name.into();
        self
    }

    /// Set the working directory for tool execution.
    pub fn with_working_directory(mut self, dir: PathBuf) -> Self {
        self.working_directory = dir;
        self
    }

    /// Run the server loop, reading from and writing to the transport.
    ///
    /// Processes JSON-RPC requests until the transport is closed.
    pub async fn run<T: McpTransport>(&mut self, transport: &mut T) -> Result<(), McpError> {
        tracing::info!(server = %self.server_name, "MCP server starting");

        loop {
            let line = match transport.receive().await? {
                Some(line) => line,
                None => {
                    tracing::info!("Transport closed, shutting down");
                    break;
                }
            };

            tracing::debug!(message = %line, "Received message");

            // Distinguish requests (have "id") from notifications (no "id")
            // by parsing as generic Value first.
            let raw: Value = match serde_json::from_str(&line) {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse JSON");
                    let resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: RpcId::Number(0),
                        result: None,
                        error: Some(McpError::JsonParse(e).to_rpc_error()),
                    };
                    let json = serde_json::to_string(&resp)?;
                    transport.send(&json).await?;
                    continue;
                }
            };

            // If no "id" field, treat as notification
            if raw.get("id").is_none() {
                if let Ok(notif) = serde_json::from_value::<JsonRpcNotification>(raw) {
                    self.handle_notification(&notif);
                }
                continue;
            }

            // Parse as a request
            let request: JsonRpcRequest = match serde_json::from_value(raw) {
                Ok(req) => req,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse JSON-RPC request");
                    let resp = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: RpcId::Number(0),
                        result: None,
                        error: Some(McpError::JsonParse(e).to_rpc_error()),
                    };
                    let json = serde_json::to_string(&resp)?;
                    transport.send(&json).await?;
                    continue;
                }
            };

            let response = self.handle_request(&request).await;
            let json = serde_json::to_string(&response)?;
            tracing::debug!(response = %json, "Sending response");
            transport.send(&json).await?;
        }

        Ok(())
    }

    /// Handle a single JSON-RPC request and produce a response.
    pub async fn handle_request(&mut self, request: &JsonRpcRequest) -> JsonRpcResponse {
        let id = request.id.clone();

        match request.method.as_str() {
            "initialize" => self.handle_initialize(id, &request.params),
            "tools/list" => self.handle_list_tools(id),
            "tools/call" => self.handle_call_tool(id, &request.params).await,
            method => {
                tracing::warn!(method = %method, "Unknown method");
                let err = McpError::MethodNotFound(method.to_string());
                JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string())
            }
        }
    }

    fn handle_notification(&mut self, notif: &JsonRpcNotification) {
        match notif.method.as_str() {
            "notifications/initialized" => {
                tracing::info!("Client confirmed initialization");
            }
            "notifications/cancelled" => {
                tracing::debug!("Client cancelled a request");
            }
            method => {
                tracing::debug!(method = %method, "Unknown notification, ignoring");
            }
        }
    }

    fn handle_initialize(&mut self, id: RpcId, _params: &Option<Value>) -> JsonRpcResponse {
        tracing::info!("Handling initialize");
        self.initialized = true;

        let result = InitializeResult {
            protocol_version: PROTOCOL_VERSION.to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability { list_changed: false }),
            },
            server_info: ServerInfo {
                name: self.server_name.clone(),
                version: Some(self.server_version.clone()),
            },
        };

        match serde_json::to_value(result) {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err(e) => {
                let err = McpError::JsonParse(e);
                JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string())
            }
        }
    }

    fn handle_list_tools(&self, id: RpcId) -> JsonRpcResponse {
        tracing::debug!("Handling tools/list");

        let tools: Vec<ToolInfo> = self.registry.list().into_iter().map(ToolInfo::from).collect();
        let result = ListToolsResult { tools };

        match serde_json::to_value(result) {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err(e) => {
                let err = McpError::JsonParse(e);
                JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string())
            }
        }
    }

    async fn handle_call_tool(&self, id: RpcId, params: &Option<Value>) -> JsonRpcResponse {
        let params = match params {
            Some(p) => p,
            None => {
                let err = McpError::InvalidParams("missing params".to_string());
                return JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string());
            }
        };

        let call_params: CallToolParams = match serde_json::from_value(params.clone()) {
            Ok(p) => p,
            Err(e) => {
                let err = McpError::InvalidParams(e.to_string());
                return JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string());
            }
        };

        tracing::debug!(tool = %call_params.name, "Handling tools/call");

        let tool = match self.registry.get(&call_params.name) {
            Some(t) => t,
            None => {
                let err = McpError::ToolNotFound(call_params.name.clone());
                return JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string());
            }
        };

        let ctx = ToolContext {
            working_directory: self.working_directory.clone(),
        };

        let result = match tool.execute(call_params.arguments, &ctx).await {
            Ok(tool_result) => CallToolResult {
                content: vec![ToolContent::Text {
                    text: tool_result.content,
                }],
                is_error: tool_result.is_error,
            },
            Err(e) => CallToolResult {
                content: vec![ToolContent::Text {
                    text: e.to_string(),
                }],
                is_error: true,
            },
        };

        match serde_json::to_value(result) {
            Ok(val) => JsonRpcResponse::success(id, val),
            Err(e) => {
                let err = McpError::JsonParse(e);
                JsonRpcResponse::error(id, err.to_rpc_error().code, err.to_string())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::ChannelTransport;
    use stupid_tool_runtime::tool::EchoTool;

    fn test_registry() -> ToolRegistry {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool).unwrap();
        reg
    }

    #[tokio::test]
    async fn test_handle_initialize() {
        let mut server = McpServer::new(test_registry());
        let req = JsonRpcRequest::new(
            RpcId::Number(1),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "test-client"}
            })),
        );

        let resp = server.handle_request(&req).await;
        assert!(resp.error.is_none());
        let result: InitializeResult =
            serde_json::from_value(resp.result.unwrap()).unwrap();
        assert_eq!(result.protocol_version, PROTOCOL_VERSION);
        assert_eq!(result.server_info.name, "stupid-mcp");
    }

    #[tokio::test]
    async fn test_handle_list_tools() {
        let mut server = McpServer::new(test_registry());
        let req = JsonRpcRequest::new(RpcId::Number(2), "tools/list", None);

        let resp = server.handle_request(&req).await;
        assert!(resp.error.is_none());
        let result: ListToolsResult =
            serde_json::from_value(resp.result.unwrap()).unwrap();
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name, "echo");
    }

    #[tokio::test]
    async fn test_handle_call_tool() {
        let mut server = McpServer::new(test_registry());
        let req = JsonRpcRequest::new(
            RpcId::Number(3),
            "tools/call",
            Some(serde_json::json!({
                "name": "echo",
                "arguments": {"message": "hello mcp"}
            })),
        );

        let resp = server.handle_request(&req).await;
        assert!(resp.error.is_none());
        let result: CallToolResult =
            serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        assert_eq!(result.content.len(), 1);
        match &result.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "hello mcp"),
        }
    }

    #[tokio::test]
    async fn test_handle_call_tool_not_found() {
        let mut server = McpServer::new(test_registry());
        let req = JsonRpcRequest::new(
            RpcId::Number(4),
            "tools/call",
            Some(serde_json::json!({
                "name": "nonexistent",
                "arguments": {}
            })),
        );

        let resp = server.handle_request(&req).await;
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, error_codes::INVALID_PARAMS);
    }

    #[tokio::test]
    async fn test_handle_unknown_method() {
        let mut server = McpServer::new(test_registry());
        let req = JsonRpcRequest::new(RpcId::Number(5), "unknown/method", None);

        let resp = server.handle_request(&req).await;
        assert!(resp.error.is_some());
        let err = resp.error.unwrap();
        assert_eq!(err.code, error_codes::METHOD_NOT_FOUND);
    }

    #[tokio::test]
    async fn test_server_run_with_channel_transport() {
        let (mut client_side, mut server_side) = ChannelTransport::pair();
        let mut server = McpServer::new(test_registry());

        // Spawn server in background
        let server_handle = tokio::spawn(async move {
            server.run(&mut server_side).await
        });

        // Send initialize request
        let init_req = JsonRpcRequest::new(
            RpcId::Number(1),
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {},
                "clientInfo": {"name": "test"}
            })),
        );
        client_side
            .send(&serde_json::to_string(&init_req).unwrap())
            .await
            .unwrap();

        let resp_line = client_side.receive().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_line).unwrap();
        assert!(resp.error.is_none());

        // Send tools/call
        let call_req = JsonRpcRequest::new(
            RpcId::Number(2),
            "tools/call",
            Some(serde_json::json!({
                "name": "echo",
                "arguments": {"message": "via transport"}
            })),
        );
        client_side
            .send(&serde_json::to_string(&call_req).unwrap())
            .await
            .unwrap();

        let resp_line = client_side.receive().await.unwrap().unwrap();
        let resp: JsonRpcResponse = serde_json::from_str(&resp_line).unwrap();
        let result: CallToolResult =
            serde_json::from_value(resp.result.unwrap()).unwrap();
        assert!(!result.is_error);
        match &result.content[0] {
            ToolContent::Text { text } => assert_eq!(text, "via transport"),
        }

        // Drop client side to close the transport and let server exit
        drop(client_side);
        server_handle.await.unwrap().unwrap();
    }
}
