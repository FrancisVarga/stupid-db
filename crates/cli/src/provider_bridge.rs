//! Bridge between the CLI config and `ToolAwareLlmProvider`.
//!
//! The `LlmProviderBridge` adapter lives in `crates/tool-runtime` for reuse.
//! This module provides the CLI-specific `create_tool_aware_provider` factory
//! that reads `CliConfig` to construct the appropriate provider.

use std::sync::Arc;

use stupid_llm::provider::LlmProvider;
use stupid_llm::providers::{claude::ClaudeProvider, ollama::OllamaProvider, openai::OpenAiProvider};
use stupid_llm::LlmProviderAdapter;
use stupid_tool_runtime::provider::ToolAwareLlmProvider;
use stupid_tool_runtime::LlmProviderBridge;

use crate::config::CliConfig;

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
        Box::new(LlmProviderAdapter(llm_provider)),
        provider_name.to_string(),
    )))
}
