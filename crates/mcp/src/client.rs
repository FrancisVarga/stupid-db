//! MCP client implementation.
//!
//! Connects to an MCP server process over stdio, discovers tools,
//! and provides an adapter that implements the `Tool` trait for each
//! remote tool.

use async_trait::async_trait;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, Command};
use tokio::sync::Mutex;

use stupid_tool_runtime::tool::{Tool, ToolContext, ToolDefinition, ToolError, ToolResult};

use crate::error::McpError;
use crate::types::*;

/// An MCP client that connects to a server subprocess over stdio.
///
/// Manages the server process lifecycle and provides tool discovery
/// and invocation through the MCP protocol.
pub struct McpClient {
    child: Child,
    reader: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    writer: Arc<Mutex<tokio::process::ChildStdin>>,
    next_id: Arc<Mutex<i64>>,
    tools: HashMap<String, ToolInfo>,
}

impl McpClient {
    /// Spawn an MCP server process and connect to it.
    ///
    /// The command is launched with stdin/stdout piped for JSON-RPC communication.
    pub async fn spawn(program: &str, args: &[&str]) -> Result<Self, McpError> {
        tracing::info!(program = %program, "Spawning MCP server process");

        let mut child = Command::new(program)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpError::ServerUnavailable("Failed to capture server stdin".to_string())
        })?;
        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::ServerUnavailable("Failed to capture server stdout".to_string())
        })?;

        let mut client = Self {
            child,
            reader: Arc::new(Mutex::new(BufReader::new(stdout))),
            writer: Arc::new(Mutex::new(stdin)),
            next_id: Arc::new(Mutex::new(1)),
            tools: HashMap::new(),
        };

        client.initialize().await?;
        client.discover_tools().await?;

        Ok(client)
    }

    /// Send a JSON-RPC request and read the response.
    async fn request(&self, method: &str, params: Option<Value>) -> Result<JsonRpcResponse, McpError> {
        let id = {
            let mut next = self.next_id.lock().await;
            let id = *next;
            *next += 1;
            id
        };

        let request = JsonRpcRequest::new(RpcId::Number(id), method, params);
        let json = serde_json::to_string(&request)?;

        tracing::debug!(method = %method, id = %id, "Sending request");

        {
            let mut writer = self.writer.lock().await;
            writer.write_all(json.as_bytes()).await?;
            writer.write_all(b"\n").await?;
            writer.flush().await?;
        }

        let mut line = String::new();
        {
            let mut reader = self.reader.lock().await;
            reader.read_line(&mut line).await?;
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())?;
        Ok(response)
    }

    /// Send a JSON-RPC notification (no response expected).
    async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), McpError> {
        let notif = JsonRpcNotification::new(method, params);
        let json = serde_json::to_string(&notif)?;

        let mut writer = self.writer.lock().await;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;
        Ok(())
    }

    /// Perform MCP initialization handshake.
    async fn initialize(&mut self) -> Result<(), McpError> {
        let params = serde_json::json!({
            "protocolVersion": PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": {
                "name": "stupid-mcp-client",
                "version": env!("CARGO_PKG_VERSION")
            }
        });

        let resp = self.request("initialize", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(McpError::ServerUnavailable(err.message));
        }

        // Send initialized notification
        self.notify("notifications/initialized", None).await?;

        tracing::info!("MCP client initialized");
        Ok(())
    }

    /// Discover available tools from the server.
    async fn discover_tools(&mut self) -> Result<(), McpError> {
        let resp = self.request("tools/list", None).await?;
        if let Some(err) = resp.error {
            return Err(McpError::ServerUnavailable(err.message));
        }

        let result: ListToolsResult = serde_json::from_value(
            resp.result.ok_or(McpError::InvalidParams("missing result".to_string()))?,
        )?;

        self.tools.clear();
        for tool in result.tools {
            tracing::debug!(name = %tool.name, "Discovered tool");
            self.tools.insert(tool.name.clone(), tool);
        }

        tracing::info!(count = self.tools.len(), "Tool discovery complete");
        Ok(())
    }

    /// Call a tool on the remote MCP server.
    pub async fn call_tool(&self, name: &str, arguments: Value) -> Result<CallToolResult, McpError> {
        let params = serde_json::json!({
            "name": name,
            "arguments": arguments,
        });

        let resp = self.request("tools/call", Some(params)).await?;
        if let Some(err) = resp.error {
            return Err(McpError::ToolExecution(err.message));
        }

        let result: CallToolResult = serde_json::from_value(
            resp.result.ok_or(McpError::InvalidParams("missing result".to_string()))?,
        )?;

        Ok(result)
    }

    /// Get the list of discovered tool definitions.
    pub fn tool_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().cloned().map(ToolDefinition::from).collect()
    }

    /// Get the list of discovered tool info objects.
    pub fn tool_infos(&self) -> Vec<&ToolInfo> {
        self.tools.values().collect()
    }

    /// Create `McpTool` adapters for all discovered tools.
    ///
    /// Each `McpTool` implements the `Tool` trait and forwards calls to the
    /// remote MCP server. This allows MCP tools to be used interchangeably
    /// with local tools.
    pub fn create_tool_adapters(&self) -> Vec<McpTool> {
        self.tools
            .values()
            .map(|info| McpTool {
                info: info.clone(),
                writer: Arc::clone(&self.writer),
                reader: Arc::clone(&self.reader),
                next_id: Arc::clone(&self.next_id),
            })
            .collect()
    }

    /// Stop the MCP server process.
    pub async fn shutdown(mut self) -> Result<(), McpError> {
        tracing::info!("Shutting down MCP server process");
        let _ = self.child.kill().await;
        Ok(())
    }
}

/// A remote tool adapter that implements the `Tool` trait.
///
/// Forwards tool execution to an MCP server over stdio, allowing
/// remote MCP tools to be used like local tools.
pub struct McpTool {
    info: ToolInfo,
    writer: Arc<Mutex<tokio::process::ChildStdin>>,
    reader: Arc<Mutex<BufReader<tokio::process::ChildStdout>>>,
    next_id: Arc<Mutex<i64>>,
}

#[async_trait]
impl Tool for McpTool {
    fn definition(&self) -> ToolDefinition {
        ToolDefinition {
            name: self.info.name.clone(),
            description: self.info.description.clone(),
            input_schema: self.info.input_schema.clone(),
        }
    }

    async fn execute(&self, input: Value, _context: &ToolContext) -> Result<ToolResult, ToolError> {
        let id = {
            let mut next = self.next_id.lock().await;
            let id = *next;
            *next += 1;
            id
        };

        let request = JsonRpcRequest::new(
            RpcId::Number(id),
            "tools/call",
            Some(serde_json::json!({
                "name": self.info.name,
                "arguments": input,
            })),
        );

        let json = serde_json::to_string(&request)
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        {
            let mut writer = self.writer.lock().await;
            writer
                .write_all(json.as_bytes())
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            writer
                .write_all(b"\n")
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
            writer
                .flush()
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }

        let mut line = String::new();
        {
            let mut reader = self.reader.lock().await;
            reader
                .read_line(&mut line)
                .await
                .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;
        }

        let response: JsonRpcResponse = serde_json::from_str(line.trim())
            .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        if let Some(err) = response.error {
            return Err(ToolError::ExecutionFailed(err.message));
        }

        let result: CallToolResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| ToolError::ExecutionFailed("missing result".to_string()))?,
        )
        .map_err(|e| ToolError::ExecutionFailed(e.to_string()))?;

        // Extract text content
        let text = result
            .content
            .into_iter()
            .map(|c| match c {
                ToolContent::Text { text } => text,
            })
            .collect::<Vec<_>>()
            .join("\n");

        Ok(ToolResult {
            tool_call_id: String::new(), // Set by caller
            content: text,
            is_error: result.is_error,
        })
    }
}
