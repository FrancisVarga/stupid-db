pub mod claude;
pub mod ollama;
pub mod openai;

use stupid_core::config::{LlmConfig, OllamaConfig};

use crate::provider::{LlmError, LlmProvider};

/// Create the appropriate LLM provider based on config.
pub fn create_provider(
    llm_config: &LlmConfig,
    ollama_config: &OllamaConfig,
) -> Result<Box<dyn LlmProvider>, LlmError> {
    match llm_config.provider.as_str() {
        "openai" => {
            let api_key = llm_config
                .openai_api_key
                .as_ref()
                .ok_or_else(|| LlmError::NotConfigured("OPENAI_API_KEY not set".into()))?;
            let base_url = llm_config
                .openai_base_url
                .as_deref()
                .unwrap_or("https://api.openai.com");
            Ok(Box::new(openai::OpenAiProvider::new(
                api_key.clone(),
                llm_config.openai_model.clone(),
                base_url.to_string(),
            )))
        }
        "anthropic" | "claude" => {
            let api_key = llm_config
                .anthropic_api_key
                .as_ref()
                .ok_or_else(|| LlmError::NotConfigured("ANTHROPIC_API_KEY not set".into()))?;
            Ok(Box::new(claude::ClaudeProvider::new(
                api_key.clone(),
                llm_config.anthropic_model.clone(),
            )))
        }
        "ollama" => Ok(Box::new(ollama::OllamaProvider::new(
            ollama_config.url.clone(),
            ollama_config.model.clone(),
        ))),
        other => Err(LlmError::NotConfigured(format!(
            "unknown LLM provider: '{}'",
            other
        ))),
    }
}
