use crate::conversation::ConversationMessage;
use crate::stream::StreamEvent;
use crate::tool::ToolDefinition;
use async_trait::async_trait;
use futures::Stream;
use std::pin::Pin;

/// Trait for LLM providers that support tool use and streaming.
///
/// This trait lives in tool-runtime (not in crates/llm) because it's
/// defined by the consumer (the agentic loop), not the provider.
/// Implementations live in crates/llm or adapter crates.
#[async_trait]
pub trait ToolAwareLlmProvider: Send + Sync {
    /// Stream a response from the LLM with tool definitions available.
    async fn stream_with_tools(
        &self,
        messages: Vec<ConversationMessage>,
        system_prompt: Option<String>,
        tools: Vec<ToolDefinition>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError>;

    /// Non-streaming convenience: collects the full response.
    async fn complete_with_tools(
        &self,
        messages: Vec<ConversationMessage>,
        system_prompt: Option<String>,
        tools: Vec<ToolDefinition>,
        temperature: f32,
        max_tokens: u32,
    ) -> Result<Vec<StreamEvent>, LlmError> {
        use futures::StreamExt;
        let stream = self
            .stream_with_tools(messages, system_prompt, tools, temperature, max_tokens)
            .await?;
        let events: Vec<_> = stream
            .collect::<Vec<_>>()
            .await
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        Ok(events)
    }

    /// Provider name for logging/debugging (e.g., "claude", "openai", "ollama")
    fn provider_name(&self) -> &str;
}

#[derive(Debug, thiserror::Error)]
pub enum LlmError {
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
    #[error("Rate limited: retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },
    #[error("Authentication failed")]
    AuthError,
    #[error("Stream error: {0}")]
    StreamError(String),
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Mock LLM provider for testing the agentic loop without real API calls.
#[cfg(any(test, feature = "test-utils"))]
pub mod mock {
    use super::*;
    use crate::stream::StopReason;
    use futures::stream;
    use std::sync::Mutex;

    /// A mock provider that returns pre-configured responses.
    pub struct MockLlmProvider {
        responses: Mutex<Vec<Vec<StreamEvent>>>,
    }

    impl MockLlmProvider {
        pub fn new() -> Self {
            Self {
                responses: Mutex::new(Vec::new()),
            }
        }

        /// Queue a response that will be returned on the next call.
        pub fn queue_response(&self, events: Vec<StreamEvent>) {
            self.responses.lock().unwrap().push(events);
        }

        /// Queue a simple text response.
        pub fn queue_text(&self, text: &str) {
            self.queue_response(vec![
                StreamEvent::TextDelta {
                    text: text.to_string(),
                },
                StreamEvent::MessageEnd {
                    stop_reason: StopReason::EndTurn,
                },
            ]);
        }
    }

    #[async_trait]
    impl ToolAwareLlmProvider for MockLlmProvider {
        async fn stream_with_tools(
            &self,
            _messages: Vec<ConversationMessage>,
            _system_prompt: Option<String>,
            _tools: Vec<ToolDefinition>,
            _temperature: f32,
            _max_tokens: u32,
        ) -> Result<Pin<Box<dyn Stream<Item = Result<StreamEvent, LlmError>> + Send>>, LlmError>
        {
            let events = self
                .responses
                .lock()
                .unwrap()
                .pop()
                .unwrap_or_else(|| {
                    vec![StreamEvent::MessageEnd {
                        stop_reason: StopReason::EndTurn,
                    }]
                });
            Ok(Box::pin(stream::iter(events.into_iter().map(Ok))))
        }

        fn provider_name(&self) -> &str {
            "mock"
        }
    }
}
