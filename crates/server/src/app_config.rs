//! Application configuration builders.
//!
//! Constructs the LLM, embedding, and agent subsystems from `Config`.

use std::sync::Arc;

use tracing::info;

use stupid_tool_runtime::permission::{PermissionLevel, PermissionPolicy, PolicyChecker};
use stupid_tool_runtime::{
    AgenticLoop, BashExecuteTool, FileReadTool, FileWriteTool, GraphQueryTool, LlmProviderBridge,
    PermissionChecker, RuleEvaluateTool, RuleListTool, ToolRegistry,
};

/// Load configuration from `.env` and environment variables.
pub fn load_config() -> stupid_core::Config {
    stupid_core::config::load_dotenv();
    stupid_core::Config::from_env()
}

/// Build the agent executor from config, loading agents from .claude/agents/.
pub fn build_agent_executor(config: &stupid_core::Config) -> Option<stupid_agent::AgentExecutor> {
    // Look for agents directory relative to the data dir or current directory
    let agents_dir = std::env::var("AGENTS_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| {
            std::path::PathBuf::from("agents/stupid-db-claude-code/agents")
        });

    if !agents_dir.exists() {
        info!(
            "Agents directory not found at {} — agent system disabled",
            agents_dir.display()
        );
        return None;
    }

    let agents = match stupid_agent::config::load_agents(&agents_dir) {
        Ok(agents) if agents.is_empty() => {
            info!(
                "No agent configs found in {} — agent system disabled",
                agents_dir.display()
            );
            return None;
        }
        Ok(agents) => {
            info!(
                "Loaded {} agent configs from {}",
                agents.len(),
                agents_dir.display()
            );
            agents
        }
        Err(e) => {
            tracing::warn!("Failed to load agents: {} — agent system disabled", e);
            return None;
        }
    };

    // Create LLM provider for agents (reuse existing LLM config)
    let provider = match stupid_llm::providers::create_provider(&config.llm, &config.ollama) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "Failed to create LLM provider for agents: {} — agent system disabled",
                e
            );
            return None;
        }
    };

    Some(stupid_agent::AgentExecutor::new(
        agents,
        provider,
        config.llm.temperature,
        config.llm.max_tokens,
    ))
}

/// Build the agentic loop from config, using `LlmProviderBridge` to wrap the
/// existing LLM provider into a `ToolAwareLlmProvider` with all 6 tools registered.
pub fn build_agentic_loop(config: &stupid_core::Config) -> Option<AgenticLoop> {
    // Create LLM provider and wrap it through the bridge
    let llm_provider = match stupid_llm::providers::create_provider(&config.llm, &config.ollama) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!(
                "Failed to create LLM provider for agentic loop: {} — agentic loop disabled",
                e
            );
            return None;
        }
    };

    let adapter = stupid_llm::LlmProviderAdapter(llm_provider);
    let provider = Arc::new(LlmProviderBridge::new(
        Box::new(adapter),
        config.llm.provider.clone(),
    ));

    // Register all 6 built-in tools
    let mut registry = ToolRegistry::new();
    registry
        .register(BashExecuteTool)
        .expect("register BashExecuteTool");
    registry
        .register(FileReadTool)
        .expect("register FileReadTool");
    registry
        .register(FileWriteTool)
        .expect("register FileWriteTool");
    registry
        .register(GraphQueryTool)
        .expect("register GraphQueryTool");
    registry
        .register(RuleListTool)
        .expect("register RuleListTool");
    registry
        .register(RuleEvaluateTool)
        .expect("register RuleEvaluateTool");

    // Server-side: auto-approve all tool executions (no interactive confirmation)
    let mut policy = PermissionPolicy::new();
    policy.default = PermissionLevel::AutoApprove;
    let permission_checker: Arc<dyn PermissionChecker> = Arc::new(PolicyChecker::new(policy));

    let agentic_loop = AgenticLoop::new(provider, Arc::new(registry), permission_checker)
        .with_temperature(config.llm.temperature)
        .with_max_tokens(config.llm.max_tokens);

    info!(
        "Agentic loop ready (provider: {}, 6 tools registered)",
        config.llm.provider
    );
    Some(agentic_loop)
}

/// Build an Embedder from config. Returns None if no embedding provider configured.
pub fn build_embedder(
    config: &stupid_core::Config,
) -> Option<Arc<dyn stupid_ingest::embedding::Embedder>> {
    use stupid_ingest::embedding::{OllamaEmbedder, OpenAiEmbedder};

    match config.embedding.provider.as_str() {
        "ollama" => {
            let embedder = OllamaEmbedder::new(
                config.ollama.url.clone(),
                config.ollama.embedding_model.clone(),
                config.embedding.dimensions as usize,
            );
            info!(
                "Embedding provider ready: ollama (model: {}, dims: {})",
                config.ollama.embedding_model, config.embedding.dimensions
            );
            Some(Arc::new(embedder))
        }
        "openai" => {
            let Some(api_key) = config.llm.openai_api_key.clone() else {
                tracing::warn!(
                    "EMBEDDING_PROVIDER=openai but OPENAI_API_KEY is empty — embedding features disabled"
                );
                return None;
            };
            let embedder = OpenAiEmbedder::new(
                api_key,
                "text-embedding-3-small".to_string(),
                config.llm.openai_base_url.clone(),
                config.embedding.dimensions as usize,
            );
            info!(
                "Embedding provider ready: openai (dims: {})",
                config.embedding.dimensions
            );
            Some(Arc::new(embedder))
        }
        other => {
            tracing::warn!(
                "Unknown embedding provider '{}' — embedding features disabled",
                other
            );
            None
        }
    }
}
