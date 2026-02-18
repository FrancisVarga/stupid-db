//! HTTP client for connecting to the stupid-db server in remote mode.
//!
//! When `--server` is set, the CLI becomes a thin client that delegates
//! to the server's session and streaming endpoints. This means both the
//! CLI and the dashboard share the same session store.

use anyhow::{bail, Context, Result};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use stupid_tool_runtime::stream::StreamEvent;

/// Client for the stupid-db server REST + SSE API.
pub struct ServerClient {
    base_url: String,
    http: reqwest::Client,
}

/// Minimal session info returned by the server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Request body for creating a session.
#[derive(Serialize)]
struct CreateSessionBody {
    name: Option<String>,
}

/// Request body for streaming.
#[derive(Serialize)]
struct StreamRequestBody {
    task: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    system_prompt: Option<String>,
    max_iterations: usize,
}

#[allow(dead_code)]
impl ServerClient {
    /// Create a new server client.
    pub fn new(base_url: &str) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        let http = reqwest::Client::new();
        Self { base_url, http }
    }

    /// Check if the server is reachable.
    pub async fn health_check(&self) -> Result<()> {
        let url = format!("{}/sessions", self.base_url);
        self.http
            .get(&url)
            .timeout(std::time::Duration::from_secs(3))
            .send()
            .await
            .context("server not reachable")?;
        Ok(())
    }

    /// List all sessions on the server.
    pub async fn list_sessions(&self) -> Result<Vec<SessionInfo>> {
        let url = format!("{}/sessions", self.base_url);
        let resp = self
            .http
            .get(&url)
            .send()
            .await
            .context("failed to list sessions")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("server returned {}: {}", status, body);
        }

        resp.json().await.context("failed to parse sessions list")
    }

    /// Create a new session on the server.
    pub async fn create_session(&self, name: Option<&str>) -> Result<SessionInfo> {
        let url = format!("{}/sessions", self.base_url);
        let resp = self
            .http
            .post(&url)
            .json(&CreateSessionBody {
                name: name.map(|s| s.to_string()),
            })
            .send()
            .await
            .context("failed to create session")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("server returned {}: {}", status, body);
        }

        resp.json().await.context("failed to parse created session")
    }

    /// Stream an agentic loop response from the server via SSE.
    ///
    /// Calls `POST /sessions/{id}/stream` and yields `StreamEvent`s as
    /// they arrive. The caller should render each event via `Terminal::display_event()`.
    pub async fn stream(
        &self,
        session_id: &str,
        task: &str,
        system_prompt: Option<&str>,
        max_iterations: usize,
    ) -> Result<impl futures::Stream<Item = Result<StreamEvent>>> {
        let url = format!("{}/sessions/{}/stream", self.base_url, session_id);
        let resp = self
            .http
            .post(&url)
            .json(&StreamRequestBody {
                task: task.to_string(),
                system_prompt: system_prompt.map(|s| s.to_string()),
                max_iterations,
            })
            .send()
            .await
            .context("failed to start stream")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("server returned {}: {}", status, body);
        }

        // Parse the SSE byte stream into StreamEvent items.
        // Each SSE message has format: "data: <json>\n\n"
        let byte_stream = resp.bytes_stream();

        // Buffer SSE lines and emit parsed events
        let event_stream = SseParser::new(byte_stream);

        Ok(event_stream)
    }
}

/// Parses an SSE byte stream into `StreamEvent` items.
///
/// SSE format:
/// ```text
/// data: {"TextDelta":{"text":"Hello"}}
///
/// data: {"MessageEnd":{"stop_reason":"EndTurn"}}
///
/// ```
struct SseParser<S> {
    inner: S,
    buffer: String,
}

impl<S> SseParser<S> {
    fn new(inner: S) -> Self {
        Self {
            inner,
            buffer: String::new(),
        }
    }
}

impl<S> futures::Stream for SseParser<S>
where
    S: futures::Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Unpin,
{
    type Item = Result<StreamEvent>;

    fn poll_next(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        loop {
            // Check if buffer already contains a complete SSE event
            if let Some(event) = self.try_parse_event() {
                return std::task::Poll::Ready(Some(event));
            }

            // Read more data from the byte stream
            match self.as_mut().project_inner().poll_next_unpin(cx) {
                std::task::Poll::Ready(Some(Ok(bytes))) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.buffer.push_str(&text);
                    // Loop back to try parsing again
                }
                std::task::Poll::Ready(Some(Err(e))) => {
                    return std::task::Poll::Ready(Some(Err(e.into())));
                }
                std::task::Poll::Ready(None) => {
                    // Stream ended — try to parse any remaining buffered data
                    if self.buffer.trim().is_empty() {
                        return std::task::Poll::Ready(None);
                    }
                    // Try one last parse
                    if let Some(event) = self.try_parse_event() {
                        return std::task::Poll::Ready(Some(event));
                    }
                    return std::task::Poll::Ready(None);
                }
                std::task::Poll::Pending => {
                    return std::task::Poll::Pending;
                }
            }
        }
    }
}

impl<S> SseParser<S> {
    /// Try to extract one SSE event from the buffer.
    fn try_parse_event(&mut self) -> Option<Result<StreamEvent>> {
        // Look for "data: " lines followed by a blank line (or another data line)
        loop {
            let line_end = self.buffer.find('\n')?;
            let line = self.buffer[..line_end].trim_end_matches('\r').to_string();

            // Consume the line from the buffer
            self.buffer = self.buffer[line_end + 1..].to_string();

            if line.is_empty() {
                // Blank line — skip (SSE event delimiter)
                continue;
            }

            if let Some(data) = line.strip_prefix("data:") {
                let data = data.trim();
                if data.is_empty() {
                    continue;
                }

                // Parse JSON into StreamEvent
                match serde_json::from_str::<StreamEvent>(data) {
                    Ok(event) => return Some(Ok(event)),
                    Err(e) => {
                        tracing::debug!(data = %data, error = %e, "Failed to parse SSE data as StreamEvent");
                        // Skip unparseable events (e.g. "[DONE]" or comments)
                        continue;
                    }
                }
            }

            // Skip non-data lines (event:, id:, retry:, comments)
        }
    }

    /// Access the inner stream for polling.
    fn project_inner(&mut self) -> &mut S {
        &mut self.inner
    }
}
