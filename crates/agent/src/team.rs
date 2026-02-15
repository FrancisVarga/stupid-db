use std::collections::HashMap;
use std::time::Instant;

use futures::future::join_all;
use tracing::{info, warn};

use crate::executor::AgentExecutor;
use crate::types::{ExecutionStatus, StrategyInfo, TeamResponse, TeamStrategy};

/// Executes tasks with coordinated teams of agents.
pub struct TeamExecutor;

impl TeamExecutor {
    /// Get agents for a given strategy.
    pub fn agents_for_strategy(strategy: TeamStrategy) -> Vec<&'static str> {
        match strategy {
            TeamStrategy::ArchitectOnly => vec!["architect"],
            TeamStrategy::LeadsOnly => vec![
                "architect",
                "backend-lead",
                "frontend-lead",
                "data-lead",
            ],
            TeamStrategy::FullHierarchy => vec![
                "architect",
                "backend-lead",
                "frontend-lead",
                "data-lead",
                "compute-specialist",
                "ingest-specialist",
                "query-specialist",
                "athena-specialist",
            ],
        }
    }

    /// Execute a task with a team of agents.
    pub async fn execute(
        executor: &AgentExecutor,
        task: &str,
        strategy: TeamStrategy,
        context: Option<&serde_json::Value>,
    ) -> TeamResponse {
        let start = Instant::now();
        let agent_names = Self::agents_for_strategy(strategy);

        info!(
            strategy = ?strategy,
            agents = ?agent_names,
            "starting team execution"
        );

        // Execute all agents in parallel
        let futures: Vec<_> = agent_names
            .iter()
            .map(|name| executor.execute(name, task, context))
            .collect();

        let results = join_all(futures).await;

        // Collect outputs
        let mut outputs = HashMap::new();
        let mut has_errors = false;

        for (name, result) in agent_names.iter().zip(results) {
            match result {
                Ok(response) => {
                    outputs.insert(name.to_string(), response.output);
                }
                Err(e) => {
                    warn!(agent = name, error = %e, "agent execution failed");
                    outputs.insert(name.to_string(), format!("Error: {e}"));
                    has_errors = true;
                }
            }
        }

        let elapsed_ms = start.elapsed().as_millis() as u64;
        let status = if has_errors {
            ExecutionStatus::Partial
        } else {
            ExecutionStatus::Success
        };

        info!(strategy = ?strategy, elapsed_ms, status = ?status, "team execution complete");

        TeamResponse {
            task: task.to_string(),
            strategy,
            agents_used: agent_names.iter().map(|s| s.to_string()).collect(),
            status,
            outputs,
            execution_time_ms: elapsed_ms,
        }
    }

    /// List available strategies.
    pub fn strategies() -> Vec<StrategyInfo> {
        vec![
            StrategyInfo {
                name: TeamStrategy::ArchitectOnly,
                agents: vec!["architect".into()],
                description: "Single architect agent for design decisions".into(),
            },
            StrategyInfo {
                name: TeamStrategy::LeadsOnly,
                agents: vec![
                    "architect".into(),
                    "backend-lead".into(),
                    "frontend-lead".into(),
                    "data-lead".into(),
                ],
                description: "Architect + domain leads for coordinated work".into(),
            },
            StrategyInfo {
                name: TeamStrategy::FullHierarchy,
                agents: Self::agents_for_strategy(TeamStrategy::FullHierarchy)
                    .iter()
                    .map(|s| s.to_string())
                    .collect(),
                description: "Full team with all specialists".into(),
            },
        ]
    }
}
