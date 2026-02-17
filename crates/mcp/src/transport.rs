//! MCP transport layer.
//!
//! Defines the `McpTransport` trait for sending/receiving JSON-RPC messages,
//! and provides a `StdioTransport` implementation for stdio-based communication.

use async_trait::async_trait;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::error::McpError;

/// Trait for MCP message transport.
///
/// Implementations handle the wire format (newline-delimited JSON) over
/// different channels (stdio, SSE, WebSocket, etc.).
#[async_trait]
pub trait McpTransport: Send + Sync {
    /// Read the next JSON-RPC message line from the transport.
    /// Returns `None` when the transport is closed.
    async fn receive(&mut self) -> Result<Option<String>, McpError>;

    /// Write a JSON-RPC message line to the transport.
    async fn send(&mut self, message: &str) -> Result<(), McpError>;
}

/// Stdio-based transport using newline-delimited JSON.
///
/// Reads from stdin, writes to stdout. Each message is a single JSON
/// object terminated by a newline character.
pub struct StdioTransport {
    reader: BufReader<tokio::io::Stdin>,
    writer: tokio::io::Stdout,
}

impl StdioTransport {
    /// Create a new stdio transport.
    pub fn new() -> Self {
        Self {
            reader: BufReader::new(tokio::io::stdin()),
            writer: tokio::io::stdout(),
        }
    }
}

impl Default for StdioTransport {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn receive(&mut self) -> Result<Option<String>, McpError> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;
        if bytes_read == 0 {
            return Ok(None); // EOF
        }
        let trimmed = line.trim().to_string();
        if trimmed.is_empty() {
            // Skip empty lines, try again
            return self.receive().await;
        }
        Ok(Some(trimmed))
    }

    async fn send(&mut self, message: &str) -> Result<(), McpError> {
        self.writer.write_all(message.as_bytes()).await?;
        self.writer.write_all(b"\n").await?;
        self.writer.flush().await?;
        Ok(())
    }
}

/// In-memory transport for testing, backed by channel pairs.
pub struct ChannelTransport {
    rx: tokio::sync::mpsc::Receiver<String>,
    tx: tokio::sync::mpsc::Sender<String>,
}

impl ChannelTransport {
    /// Create a pair of connected transports for testing.
    ///
    /// Messages sent on one transport are received by the other.
    pub fn pair() -> (Self, Self) {
        let (tx_a, rx_b) = tokio::sync::mpsc::channel(32);
        let (tx_b, rx_a) = tokio::sync::mpsc::channel(32);
        (
            Self { rx: rx_a, tx: tx_a },
            Self { rx: rx_b, tx: tx_b },
        )
    }
}

#[async_trait]
impl McpTransport for ChannelTransport {
    async fn receive(&mut self) -> Result<Option<String>, McpError> {
        match self.rx.recv().await {
            Some(msg) => Ok(Some(msg)),
            None => Ok(None),
        }
    }

    async fn send(&mut self, message: &str) -> Result<(), McpError> {
        self.tx
            .send(message.to_string())
            .await
            .map_err(|e| McpError::Transport(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_channel_transport_pair() {
        let (mut a, mut b) = ChannelTransport::pair();

        a.send("hello from a").await.unwrap();
        let msg = b.receive().await.unwrap();
        assert_eq!(msg, Some("hello from a".to_string()));

        b.send("hello from b").await.unwrap();
        let msg = a.receive().await.unwrap();
        assert_eq!(msg, Some("hello from b".to_string()));
    }

    #[tokio::test]
    async fn test_channel_transport_closed() {
        let (mut a, b) = ChannelTransport::pair();
        drop(b);
        let result = a.receive().await.unwrap();
        assert_eq!(result, None);
    }
}
