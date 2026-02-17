//! Bridge between the CLI config and `ToolAwareLlmProvider`.
//!
//! The existing `crates/llm` providers implement the simple `LlmProvider` trait
//! (non-streaming, no tool support). The agentic loop requires `ToolAwareLlmProvider`
//! (streaming + tool definitions). This module provides a bridge adapter that
//! wraps any `LlmProvider` into a `ToolAwareLlmProvider` by simulating streaming
//! from the non-streaming response.
//!
//! As native streaming providers are implemented in `crates/llm`, they should be
//! used directly instead of this bridge.

use async_trait::async_trait;
use futures::stream;
use std::pin::Pin;
use std::sync::Arc;

use stupid_llm::provider::LlmProvider;
use stupid_llm::providers::{claude::ClaudeProvider, ollama::OllamaProvider, openai::OpenAiProvider};
use stupid_tool_runtime::conversation::ConversationMessage;
use stupid_tool_runtime::provider::{LlmError, ToolAwareLlmProvider};
use stupid_tool_runtime::stream::{StopReason, StreamEvent};
use stupid_tool_runtime::tool::ToolDefinition;

use crate::config::CliConfig;

/// Wraps a simple `LlmProvider` into a `ToolAwareLlmProvider`.
///
/// Converts the conversation format and simulates streaming by returning
/// the full response as a single TextDelta event.
pub struct LlmProviderBridge {
    inner: Box<dyn LlmProvider>,
    name: String,
}

impl LlmProviderBridge {
    /// Create a bridge from any `LlmProvider`.
    pub fn new(inner: Box<dyn LlmProvider>, name: String) -> Self {
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
        // Convert ConversationMessage to stupid_llm::Message
        let mut llm_messages = Vec::new();

        if let Some(sys) = system_prompt {
            llm_messages.push(stupid_llm::Message {
                role: stupid_llm::Role::System,
                content: sys,
            });
        }

        for msg in &messages {
            match msg {
                ConversationMessage::User(text) => {
                    llm_messages.push(stupid_llm::Message {
                        role: stupid_llm::Role::User,
                        content: text.clone(),
                    });
                }
                ConversationMessage::Assistant(content) => {
                    if let Some(text) = &content.text {
                        llm_messages.push(stupid_llm::Message {
                            role: stupid_llm::Role::Assistant,
                            content: text.clone(),
                        });
                    }
                }
                ConversationMessage::ToolResult(result) => {
                    // Encode tool results as user messages for simple providers
                    llm_messages.push(stupid_llm::Message {
                        role: stupid_llm::Role::User,
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
            .map_err(|e| LlmError::Other(anyhow::anyhow!("{}", e)))?;

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

/// Create a `ToolAwareLlmProvider` from CLI config and arguments.
pub fn create_tool_aware_provider(
    config: &CliConfig,
    provider_name: &str,
    model: &str,
    api_key: Option<&str>,
) -> anyhow::Result<Arc<dyn ToolAwareLlmProvider>> {
    let llm_provider: Box<dyn LlmProvider> = match provider_name {
        "claude" | "anthropic" => {
            let key = api_key
                .map(String::from)
                .or_else(|| config.resolve_api_key(provider_name, None))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No API key for Claude. Set ANTHROPIC_API_KEY or pass --api-key"
                    )
                })?;
            Box::new(ClaudeProvider::new(key, model.to_string()))
        }
        "openai" => {
            let key = api_key
                .map(String::from)
                .or_else(|| config.resolve_api_key(provider_name, None))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "No API key for OpenAI. Set OPENAI_API_KEY or pass --api-key"
                    )
                })?;
            Box::new(OpenAiProvider::new(
                key,
                model.to_string(),
                config.openai_base_url.clone(),
            ))
        }
        "ollama" => Box::new(OllamaProvider::new(
            config.ollama_url.clone(),
            model.to_string(),
        )),
        other => anyhow::bail!("Unknown provider '{}'. Supported: claude, openai, ollama", other),
    };

    Ok(Arc::new(LlmProviderBridge::new(
        llm_provider,
        provider_name.to_string(),
    )))
}
