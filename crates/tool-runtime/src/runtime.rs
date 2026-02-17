use crate::conversation::{AssistantContent, Conversation};
use crate::permission::{PermissionChecker, PermissionDecision};
use crate::provider::{LlmError, ToolAwareLlmProvider};
use crate::registry::ToolRegistry;
use crate::stream::{StopReason, StreamEvent};
use crate::tool::{ToolCall, ToolContext, ToolResult};
use futures::StreamExt;
use std::sync::Arc;
use tracing::{debug, info, warn};

/// The core agentic loop that orchestrates LLM ↔ Tool execution.
///
/// Flow: User → LLM → ToolCalls → Execute → Results → LLM → ... → Final Text
pub struct AgenticLoop {
    provider: Arc<dyn ToolAwareLlmProvider>,
    registry: Arc<ToolRegistry>,
    permission_checker: Arc<dyn PermissionChecker>,
    max_iterations: usize,
    temperature: f32,
    max_tokens: u32,
}

impl AgenticLoop {
    pub fn new(
        provider: Arc<dyn ToolAwareLlmProvider>,
        registry: Arc<ToolRegistry>,
        permission_checker: Arc<dyn PermissionChecker>,
    ) -> Self {
        Self {
            provider,
            registry,
            permission_checker,
            max_iterations: 10,
            temperature: 0.0,
            max_tokens: 4096,
        }
    }

    pub fn with_max_iterations(mut self, max: usize) -> Self {
        self.max_iterations = max;
        self
    }

    pub fn with_temperature(mut self, temp: f32) -> Self {
        self.temperature = temp;
        self
    }

    pub fn with_max_tokens(mut self, max: u32) -> Self {
        self.max_tokens = max;
        self
    }

    /// Run a single user turn through the agentic loop.
    /// Returns all stream events and the final conversation state.
    pub async fn run(
        &self,
        conversation: &mut Conversation,
        user_message: String,
        tool_context: &ToolContext,
    ) -> Result<Vec<StreamEvent>, AgenticLoopError> {
        conversation.add_user_message(user_message);
        let mut all_events = Vec::new();

        for iteration in 0..self.max_iterations {
            debug!(iteration, "Starting agentic loop iteration");

            // Get tool definitions
            let tools = self.registry.list();

            // Stream LLM response
            let mut stream = self
                .provider
                .stream_with_tools(
                    conversation.messages().to_vec(),
                    conversation.system_prompt().map(String::from),
                    tools,
                    self.temperature,
                    self.max_tokens,
                )
                .await
                .map_err(AgenticLoopError::LlmError)?;

            // Collect events from this turn
            let mut text_parts = Vec::new();
            let mut tool_calls: Vec<ToolCall> = Vec::new();
            let mut current_tool_args = String::new();
            let mut current_tool_id = String::new();
            let mut current_tool_name = String::new();
            let mut stop_reason = StopReason::EndTurn;

            while let Some(event_result) = stream.next().await {
                let event = event_result.map_err(AgenticLoopError::LlmError)?;
                match &event {
                    StreamEvent::TextDelta { text } => {
                        text_parts.push(text.clone());
                    }
                    StreamEvent::ToolCallStart { id, name } => {
                        current_tool_id = id.clone();
                        current_tool_name = name.clone();
                        current_tool_args.clear();
                    }
                    StreamEvent::ToolCallDelta { arguments_delta, .. } => {
                        current_tool_args.push_str(arguments_delta);
                    }
                    StreamEvent::ToolCallEnd { .. } => {
                        let input: serde_json::Value =
                            serde_json::from_str(&current_tool_args).unwrap_or_default();
                        tool_calls.push(ToolCall {
                            id: current_tool_id.clone(),
                            name: current_tool_name.clone(),
                            input,
                        });
                    }
                    StreamEvent::MessageEnd { stop_reason: reason } => {
                        stop_reason = reason.clone();
                    }
                    StreamEvent::Error { message } => {
                        warn!(message, "Stream error");
                    }
                }
                all_events.push(event);
            }

            // Add assistant response to conversation
            let text = if text_parts.is_empty() {
                None
            } else {
                Some(text_parts.join(""))
            };
            conversation.add_assistant_response(AssistantContent {
                text,
                tool_calls: tool_calls.clone(),
            });

            // If no tool calls, we're done
            if tool_calls.is_empty() || stop_reason == StopReason::EndTurn {
                info!(iteration, "Agentic loop complete (no tool calls or end turn)");
                break;
            }

            // Execute tool calls
            info!(count = tool_calls.len(), "Executing tool calls");
            let results = self
                .execute_tool_calls(&tool_calls, tool_context)
                .await;

            // Add results to conversation
            for result in results {
                conversation.add_tool_result(result);
            }
        }

        Ok(all_events)
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: &[ToolCall],
        context: &ToolContext,
    ) -> Vec<ToolResult> {
        let mut results = Vec::new();

        // Execute in parallel where possible
        let mut futures = Vec::new();
        for call in tool_calls {
            let registry = self.registry.clone();
            let permission_checker = self.permission_checker.clone();
            let call = call.clone();
            let ctx_path = context.working_directory.clone();

            futures.push(async move {
                // Check permissions
                let decision = permission_checker
                    .check_permission(&call.name, &call.input)
                    .await;

                match decision {
                    PermissionDecision::Denied(reason) => ToolResult {
                        tool_call_id: call.id,
                        content: format!("Permission denied: {}", reason),
                        is_error: true,
                    },
                    PermissionDecision::NeedsConfirmation => {
                        // In the agentic loop, we can't prompt interactively.
                        // The CLI layer handles this before reaching here.
                        ToolResult {
                            tool_call_id: call.id,
                            content: "Tool requires user confirmation".to_string(),
                            is_error: true,
                        }
                    }
                    PermissionDecision::Approved => {
                        match registry.get(&call.name) {
                            Some(tool) => {
                                let tool_ctx = ToolContext {
                                    working_directory: ctx_path,
                                };
                                match tool.execute(call.input, &tool_ctx).await {
                                    Ok(mut result) => {
                                        result.tool_call_id = call.id;
                                        result
                                    }
                                    Err(e) => ToolResult {
                                        tool_call_id: call.id,
                                        content: format!("Tool error: {}", e),
                                        is_error: true,
                                    },
                                }
                            }
                            None => ToolResult {
                                tool_call_id: call.id,
                                content: format!("Unknown tool: {}", call.name),
                                is_error: true,
                            },
                        }
                    }
                }
            });
        }

        for future in futures {
            results.push(future.await);
        }

        results
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgenticLoopError {
    #[error("LLM error: {0}")]
    LlmError(#[from] LlmError),
    #[error("Max iterations ({0}) exceeded")]
    MaxIterations(usize),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::{PermissionLevel, PermissionPolicy, PolicyChecker};
    use crate::provider::mock::MockLlmProvider;
    use crate::tool::EchoTool;

    fn setup_test_loop() -> (AgenticLoop, Arc<MockLlmProvider>) {
        let provider = Arc::new(MockLlmProvider::new());
        let mut registry = ToolRegistry::new();
        registry.register(EchoTool).unwrap();

        let mut policy = PermissionPolicy::new();
        policy.default = PermissionLevel::AutoApprove;
        let checker = Arc::new(PolicyChecker::new(policy));

        let agentic_loop = AgenticLoop::new(
            provider.clone() as Arc<dyn ToolAwareLlmProvider>,
            Arc::new(registry),
            checker as Arc<dyn PermissionChecker>,
        );

        (agentic_loop, provider)
    }

    #[tokio::test]
    async fn test_simple_text_response() {
        let (agentic_loop, provider) = setup_test_loop();
        provider.queue_text("Hello, I'm an AI assistant!");

        let mut conv = Conversation::new(100_000);
        let ctx = ToolContext {
            working_directory: std::path::PathBuf::from("/tmp"),
        };

        let events = agentic_loop.run(&mut conv, "Hello".to_string(), &ctx).await.unwrap();

        assert!(!events.is_empty());
        assert!(conv.messages().len() >= 2); // user + assistant
    }

    #[tokio::test]
    async fn test_tool_call_and_response() {
        let (agentic_loop, provider) = setup_test_loop();

        // First response: tool call
        provider.queue_text("Done!"); // Second response after tool
        provider.queue_response(vec![
            StreamEvent::ToolCallStart {
                id: "call_1".to_string(),
                name: "echo".to_string(),
            },
            StreamEvent::ToolCallDelta {
                id: "call_1".to_string(),
                arguments_delta: r#"{"message": "test"}"#.to_string(),
            },
            StreamEvent::ToolCallEnd {
                id: "call_1".to_string(),
            },
            StreamEvent::MessageEnd {
                stop_reason: StopReason::ToolUse,
            },
        ]);

        let mut conv = Conversation::new(100_000);
        let ctx = ToolContext {
            working_directory: std::path::PathBuf::from("/tmp"),
        };

        let _events = agentic_loop
            .run(&mut conv, "Echo test".to_string(), &ctx)
            .await
            .unwrap();

        // Should have: user, assistant (tool call), tool result, assistant (text)
        assert!(conv.messages().len() >= 3);
    }
}
