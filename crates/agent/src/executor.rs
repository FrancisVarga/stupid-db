use std::collections::HashMap;
use std::time::Instant;

use tracing::info;

use stupid_llm::provider::{LlmError, LlmProvider, Message, Role};

use crate::config::AgentConfig;
use crate::types::{AgentResponse, ExecutionStatus};

/// Executes individual agents using an LLM provider.
pub struct AgentExecutor {
    pub agents: HashMap<String, AgentConfig>,
    provider: Box<dyn LlmProvider>,
    temperature: f32,
    max_tokens: u32,
}

impl AgentExecutor {
    pub fn new(
        agents: HashMap<String, AgentConfig>,
        provider: Box<dyn LlmProvider>,
        temperature: f32,
        max_tokens: u32,
    ) -> Self {
        Self {
            agents,
            provider,
            temperature,
            max_tokens,
        }
    }

    /// Execute a single agent with a task.
    pub async fn execute(
        &self,
        agent_name: &str,
        task: &str,
        context: Option<&serde_json::Value>,
    ) -> Result<AgentResponse, AgentExecutionError> {
        let start = Instant::now();

        let config = self
            .agents
            .get(agent_name)
            .ok_or_else(|| AgentExecutionError::AgentNotFound(agent_name.to_string()))?;

        info!(agent = agent_name, "executing agent task");

        // Build messages with agent's system prompt
        let mut system_content = config.system_prompt.clone();
        if let Some(ctx) = context {
            if !ctx.is_null() {
                system_content.push_str("\n\n## Additional Context\n");
                system_content.push_str(&serde_json::to_string_pretty(ctx).unwrap_or_default());
            }
        }

        let messages = vec![
            Message {
                role: Role::System,
                content: system_content,
            },
            Message {
                role: Role::User,
                content: task.to_string(),
            },
        ];

        let output = self
            .provider
            .complete(messages, self.temperature, self.max_tokens)
            .await
            .map_err(AgentExecutionError::LlmError)?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        info!(agent = agent_name, elapsed_ms, "agent execution complete");

        Ok(AgentResponse {
            agent_name: agent_name.to_string(),
            status: ExecutionStatus::Success,
            output,
            execution_time_ms: elapsed_ms,
            tokens_used: None,
        })
    }

    /// Execute an agent with conversation history for context continuity.
    pub async fn execute_with_history(
        &self,
        agent_name: &str,
        task: &str,
        history: &[crate::session::SessionMessage],
        context: Option<&serde_json::Value>,
        max_history: usize,
    ) -> Result<AgentResponse, AgentExecutionError> {
        let start = Instant::now();

        let config = self
            .agents
            .get(agent_name)
            .ok_or_else(|| AgentExecutionError::AgentNotFound(agent_name.to_string()))?;

        info!(agent = agent_name, history_len = history.len(), max_history, "executing agent with history");

        // Build system message
        let mut system_content = config.system_prompt.clone();
        if let Some(ctx) = context {
            if !ctx.is_null() {
                system_content.push_str("\n\n## Additional Context\n");
                system_content.push_str(&serde_json::to_string_pretty(ctx).unwrap_or_default());
            }
        }

        let mut messages = vec![Message {
            role: Role::System,
            content: system_content,
        }];

        // Filter to User + Agent/Team roles, take last max_history pairs
        let relevant: Vec<_> = history
            .iter()
            .filter(|m| matches!(
                m.role,
                crate::session::SessionMessageRole::User
                | crate::session::SessionMessageRole::Agent
                | crate::session::SessionMessageRole::Team
            ))
            .collect();

        let skip = relevant.len().saturating_sub(max_history * 2);
        for msg in relevant.into_iter().skip(skip) {
            let role = match msg.role {
                crate::session::SessionMessageRole::User => Role::User,
                crate::session::SessionMessageRole::Agent => Role::Assistant,
                crate::session::SessionMessageRole::Team => Role::Assistant,
                _ => continue,
            };

            let content = if msg.role == crate::session::SessionMessageRole::Team {
                // Concatenate team outputs for assistant context
                if let Some(ref outputs) = msg.team_outputs {
                    outputs
                        .iter()
                        .map(|(name, out)| format!("[{}]: {}", name, out))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                } else {
                    msg.content.clone()
                }
            } else {
                msg.content.clone()
            };

            messages.push(Message { role, content });
        }

        // Append current task
        messages.push(Message {
            role: Role::User,
            content: task.to_string(),
        });

        let output = self
            .provider
            .complete(messages, self.temperature, self.max_tokens)
            .await
            .map_err(AgentExecutionError::LlmError)?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        info!(agent = agent_name, elapsed_ms, "agent execution with history complete");

        Ok(AgentResponse {
            agent_name: agent_name.to_string(),
            status: ExecutionStatus::Success,
            output,
            execution_time_ms: elapsed_ms,
            tokens_used: None,
        })
    }

    /// Execute directly against the LLM with session history, no agent config needed.
    pub async fn execute_direct(
        &self,
        task: &str,
        history: &[crate::session::SessionMessage],
        context: Option<&serde_json::Value>,
        max_history: usize,
    ) -> Result<AgentResponse, AgentExecutionError> {
        let start = Instant::now();

        info!(history_len = history.len(), max_history, "executing direct (no agent)");

        let mut system_content =
            "You are a helpful AI assistant. Answer questions clearly and concisely.".to_string();
        if let Some(ctx) = context {
            if !ctx.is_null() {
                system_content.push_str("\n\n## Additional Context\n");
                system_content.push_str(&serde_json::to_string_pretty(ctx).unwrap_or_default());
            }
        }

        let mut messages = vec![Message {
            role: Role::System,
            content: system_content,
        }];

        // Reuse same history-building logic
        let relevant: Vec<_> = history
            .iter()
            .filter(|m| matches!(
                m.role,
                crate::session::SessionMessageRole::User
                | crate::session::SessionMessageRole::Agent
                | crate::session::SessionMessageRole::Team
            ))
            .collect();

        let skip = relevant.len().saturating_sub(max_history * 2);
        for msg in relevant.into_iter().skip(skip) {
            let role = match msg.role {
                crate::session::SessionMessageRole::User => Role::User,
                crate::session::SessionMessageRole::Agent
                | crate::session::SessionMessageRole::Team => Role::Assistant,
                _ => continue,
            };

            let content = if msg.role == crate::session::SessionMessageRole::Team {
                if let Some(ref outputs) = msg.team_outputs {
                    outputs
                        .iter()
                        .map(|(name, out)| format!("[{}]: {}", name, out))
                        .collect::<Vec<_>>()
                        .join("\n\n")
                } else {
                    msg.content.clone()
                }
            } else {
                msg.content.clone()
            };

            messages.push(Message { role, content });
        }

        messages.push(Message {
            role: Role::User,
            content: task.to_string(),
        });

        let output = self
            .provider
            .complete(messages, self.temperature, self.max_tokens)
            .await
            .map_err(AgentExecutionError::LlmError)?;

        let elapsed_ms = start.elapsed().as_millis() as u64;
        info!(elapsed_ms, "direct execution complete");

        Ok(AgentResponse {
            agent_name: "assistant".to_string(),
            status: ExecutionStatus::Success,
            output,
            execution_time_ms: elapsed_ms,
            tokens_used: None,
        })
    }

    /// List available agent names.
    pub fn agent_names(&self) -> Vec<String> {
        self.agents.keys().cloned().collect()
    }

    /// Check if an agent exists.
    pub fn has_agent(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AgentExecutionError {
    #[error("agent not found: {0}")]
    AgentNotFound(String),
    #[error("LLM error: {0}")]
    LlmError(LlmError),
}
