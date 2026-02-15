use serde::{Deserialize, Serialize};

/// Agent tier in the hierarchy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentTier {
    /// System architect — cross-cutting design
    Architect,
    /// Domain leads — coordinate specialists
    Lead,
    /// Domain specialists — deep expertise
    Specialist,
}

/// Metadata about an available agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInfo {
    pub name: String,
    pub tier: AgentTier,
    pub description: String,
}

/// Request to execute a single agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentRequest {
    pub agent_name: String,
    pub task: String,
    #[serde(default)]
    pub context: serde_json::Value,
}

/// Response from a single agent execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentResponse {
    pub agent_name: String,
    pub status: ExecutionStatus,
    pub output: String,
    pub execution_time_ms: u64,
    pub tokens_used: Option<u64>,
}

/// Execution status.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    Success,
    Error,
    Timeout,
    Partial,
}

/// Team execution strategy.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TeamStrategy {
    ArchitectOnly,
    LeadsOnly,
    FullHierarchy,
}

/// Request to execute a team of agents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamRequest {
    pub task: String,
    #[serde(default = "default_strategy")]
    pub strategy: TeamStrategy,
    #[serde(default)]
    pub context: serde_json::Value,
}

fn default_strategy() -> TeamStrategy {
    TeamStrategy::FullHierarchy
}

/// Response from team execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamResponse {
    pub task: String,
    pub strategy: TeamStrategy,
    pub agents_used: Vec<String>,
    pub status: ExecutionStatus,
    pub outputs: std::collections::HashMap<String, String>,
    pub execution_time_ms: u64,
}

/// Strategy metadata for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyInfo {
    pub name: TeamStrategy,
    pub agents: Vec<String>,
    pub description: String,
}
