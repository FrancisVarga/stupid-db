//! Bridge adapter: wraps a simple `SimpleLlmProvider` into a `ToolAwareLlmProvider`.
//!
//! The `SimpleLlmProvider` trait is a minimal non-streaming LLM interface defined
//! here in tool-runtime to avoid cyclic dependencies with `crates/llm`.
//! `crates/llm` provides a blanket adapter from its `LlmProvider` trait to
//! `SimpleLlmProvider`, so any existing provider works seamlessly.
//!
//! As native streaming providers are implemented in `crates/llm`, they should
//! implement `ToolAwareLlmProvider` directly instead of going through this bridge.

use async_trait::async_trait;
use futures::stream;
use std::pin::Pin;

use crate::conversation::ConversationMessage;
use crate::provider::{LlmError, ToolAwareLlmProvider};
use crate::stream::{StopReason, StreamEvent};
use crate::tool::ToolDefinition;

/// A simple chat message for non-streaming LLM providers.
#[derive(Debug, Clone)]
pub struct SimpleMessage {
    pub role: SimpleRole,
    pub content: String,
}

/// Role in a chat conversation.
#[derive(Debug, Clone)]
pub enum SimpleRole {
    System,
    User,
    Assistant,
}

/// Error type for `SimpleLlmProvider` operations.
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct BridgeError(pub String);

/// Minimal non-streaming LLM provider trait.
///
/// This trait mirrors the shape of `stupid_llm::LlmProvider` but lives in
/// tool-runtime to avoid a cyclic dependency. The `stupid-llm` crate provides
/// an `LlmProviderAdapter` wrapper so any `LlmProvider` can be used as a
/// `SimpleLlmProvider`.
#[async_trait]
pub trait SimpleLlmProvider: Send + Sync {
    /// Send a chat completion request and return the assistant's response text.
    async fn complete(
        &self,
        messages: Vec<SimpleMessage>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<String, BridgeError>;
}

/// Wraps a `SimpleLlmProvider` into a `ToolAwareLlmProvider`.
///
/// Converts the conversation format and simulates streaming by returning
/// the full response as a single TextDelta event.
pub struct LlmProviderBridge {
    inner: Box<dyn SimpleLlmProvider>,
    name: String,
}

impl LlmProviderBridge {
    /// Create a bridge from any `SimpleLlmProvider`.
    pub fn new(inner: Box<dyn SimpleLlmProvider>, name: String) -> Self {
        Self { inner, name }
    }
}

#[async_trait]
impl ToolAwareLlmProvider for LlmProviderBridge {
    async fn stream_with_tools(
        &self,
        messages: Vec<ConversationMessage>,
        system_prompt: Option<String>,
        _tools: Vec<ToolDefinition>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<
        Pin<
            Box<
                dyn futures::Stream<Item = Result<StreamEvent, LlmError>>
                    + Send,
            >,
        >,
        LlmError,
    > {
        // Convert ConversationMessage to SimpleMessage
        let mut llm_messages = Vec::new();

        if let Some(sys) = system_prompt {
            llm_messages.push(SimpleMessage {
                role: SimpleRole::System,
                content: sys,
            });
        }

        for msg in &messages {
            match msg {
                ConversationMessage::User(text) => {
                    llm_messages.push(SimpleMessage {
                        role: SimpleRole::User,
                        content: text.clone(),
                    });
                }
                ConversationMessage::Assistant(content) => {
                    if let Some(text) = &content.text {
                        llm_messages.push(SimpleMessage {
                            role: SimpleRole::Assistant,
                            content: text.clone(),
                        });
                    }
                }
                ConversationMessage::ToolResult(result) => {
                    // Encode tool results as user messages for simple providers
                    llm_messages.push(SimpleMessage {
                        role: SimpleRole::User,
                        content: format!("[Tool Result: {}]", result.content),
                    });
                }
            }
        }

        // Call the underlying provider (non-streaming)
        let response = self
            .inner
            .complete(llm_messages, temperature, max_tokens)
            .await
            .map_err(|e| LlmError::Other(anyhow::anyhow!("{}", e.0)))?;

        // Simulate streaming with a single text delta + message end
        let events = vec![
            Ok(StreamEvent::TextDelta {
                text: response,
            }),
            Ok(StreamEvent::MessageEnd {
                stop_reason: StopReason::EndTurn,
            }),
        ];

        Ok(Box::pin(stream::iter(events)))
    }

    fn provider_name(&self) -> &str {
        &self.name
    }
}
