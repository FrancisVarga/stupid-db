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
