//! MCP (Model Context Protocol) implementation for stupid-db.
//!
//! This crate implements the MCP protocol over JSON-RPC 2.0, enabling
//! tool interoperability between LLM agents and the stupid-db tool system.
//!
//! # Architecture
//!
//! - **types**: JSON-RPC 2.0 and MCP-specific protocol types
//! - **transport**: Pluggable transport layer (stdio, channels)
//! - **server**: MCP server wrapping a `ToolRegistry`
//! - **client**: MCP client connecting to server subprocesses
//! - **error**: Unified error types
//!
//! # Usage
//!
//! ## Server
//! ```no_run
//! use stupid_mcp::server::McpServer;
//! use stupid_mcp::transport::StdioTransport;
//! use stupid_tool_runtime::ToolRegistry;
//!
//! # async fn example() {
//! let registry = ToolRegistry::new();
//! let mut server = McpServer::new(registry);
//! let mut transport = StdioTransport::new();
//! server.run(&mut transport).await.unwrap();
//! # }
//! ```
//!
//! ## Client
//! ```no_run
//! use stupid_mcp::client::McpClient;
//!
//! # async fn example() {
//! let client = McpClient::spawn("my-mcp-server", &[]).await.unwrap();
//! let tools = client.tool_definitions();
//! # }
//! ```

pub mod types;
pub mod transport;
pub mod server;
pub mod client;
pub mod error;

pub use types::*;
pub use transport::{McpTransport, StdioTransport, ChannelTransport};
pub use server::McpServer;
pub use client::{McpClient, McpTool};
pub use error::McpError;
