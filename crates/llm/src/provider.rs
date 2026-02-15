use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// A chat message for the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    System,
    User,
    Assistant,
}

/// Trait for LLM providers — each backend implements this.
#[async_trait]
pub trait LlmProvider: Send + Sync {
    /// Send a chat completion request and return the assistant's response text.
    async fn complete(
        &self,
        messages: Vec<Message>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, LlmError>;
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("API error: {status} — {body}")]
    ApiError { status: u16, body: String },
    #[error("failed to parse response: {0}")]
    ParseError(String),
    #[error("provider not configured: {0}")]
    NotConfigured(String),
}
