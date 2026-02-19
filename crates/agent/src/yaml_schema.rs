//! YAML-based agent configuration schema.
//!
//! Supports 4 LLM providers: Anthropic, OpenAI, Gemini, and Ollama.
//! Agent configs live in `data/agents/*.yaml` and are loaded at startup.
//!
//! API keys are referenced by environment variable name (`api_key_env`),
//! never stored directly in config files.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::types::AgentTier;

// ── Top-level agent config ────────────────────────────────────────

/// Complete YAML agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentYamlConfig {
    /// Unique agent name (required).
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// Agent tier in the hierarchy.
    #[serde(default)]
    pub tier: AgentTier,

    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Optional grouping for UI organization.
    pub group: Option<String>,

    /// LLM provider configuration (required).
    pub provider: ProviderConfig,

    /// Execution parameters (temperature, tokens, etc.).
    #[serde(default)]
    pub execution: ExecutionConfig,

    /// The agent's system prompt / personality.
    #[serde(default)]
    pub system_prompt: String,

    /// Reusable prompt fragments.
    #[serde(default)]
    pub skills: Vec<SkillConfig>,
}

// ── Provider configuration (tagged enum) ──────────────────────────

/// Provider-specific configuration, tagged by `type` field.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ProviderConfig {
    Anthropic(AnthropicConfig),
    #[serde(rename = "openai")]
    OpenAi(OpenAiConfig),
    Gemini(GeminiConfig),
    Ollama(OllamaConfig),
}

/// Anthropic (Claude) provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicConfig {
    /// Model identifier, e.g. "claude-sonnet-4-5-20250929".
    pub model: String,

    /// Environment variable holding the API key.
    pub api_key_env: Option<String>,

    /// Optional custom base URL.
    pub base_url: Option<String>,

    /// Provider-specific overrides.
    #[serde(default)]
    pub anthropic: AnthropicSpecific,
}

/// Anthropic-specific settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnthropicSpecific {
    /// API version header (default: "2023-06-01").
    #[serde(default = "default_anthropic_version")]
    pub version: String,
}

impl Default for AnthropicSpecific {
    fn default() -> Self {
        Self {
            version: default_anthropic_version(),
        }
    }
}

fn default_anthropic_version() -> String {
    "2023-06-01".to_string()
}

/// OpenAI provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiConfig {
    /// Model identifier, e.g. "gpt-4o".
    pub model: String,

    /// Environment variable holding the API key.
    pub api_key_env: Option<String>,

    /// Base URL (default: "https://api.openai.com"). Useful for Azure or proxies.
    pub base_url: Option<String>,

    /// Provider-specific overrides.
    #[serde(default)]
    pub openai: OpenAiSpecific,
}

/// OpenAI-specific settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OpenAiSpecific {
    /// Response format: "json_object" or "text".
    pub response_format: Option<String>,

    /// OpenAI organization ID.
    pub organization: Option<String>,
}

/// Google Gemini provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeminiConfig {
    /// Model identifier, e.g. "gemini-2.0-flash".
    pub model: String,

    /// Environment variable holding the API key.
    pub api_key_env: Option<String>,

    /// Optional custom base URL.
    pub base_url: Option<String>,

    /// Provider-specific overrides.
    #[serde(default)]
    pub gemini: GeminiSpecific,
}

/// Gemini-specific settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeminiSpecific {
    /// Safety thresholds keyed by category.
    #[serde(default)]
    pub safety_settings: HashMap<String, String>,

    /// Generation parameters (candidate_count, stop_sequences, etc.).
    #[serde(default)]
    pub generation_config: HashMap<String, serde_json::Value>,
}

/// Ollama (local) provider settings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OllamaConfig {
    /// Model identifier, e.g. "llama3.1:70b".
    pub model: String,

    /// Ollama server URL (default: "http://localhost:11434").
    pub base_url: Option<String>,

    /// Provider-specific overrides.
    #[serde(default)]
    pub ollama: OllamaSpecific,
}

/// Ollama-specific settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OllamaSpecific {
    /// Context window size.
    pub num_ctx: Option<u32>,

    /// Number of GPUs to use.
    pub num_gpu: Option<u32>,
}

// ── Execution settings ────────────────────────────────────────────

/// Execution parameters controlling LLM behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionConfig {
    /// Sampling temperature (0.0–2.0).
    #[serde(default = "default_temperature")]
    pub temperature: f32,

    /// Maximum output tokens.
    #[serde(default = "default_max_tokens")]
    pub max_tokens: u32,

    /// Nucleus sampling threshold.
    pub top_p: Option<f32>,

    /// Per-request timeout in seconds.
    #[serde(default = "default_timeout")]
    pub timeout_seconds: u32,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            temperature: default_temperature(),
            max_tokens: default_max_tokens(),
            top_p: None,
            timeout_seconds: default_timeout(),
        }
    }
}

fn default_temperature() -> f32 {
    0.1
}
fn default_max_tokens() -> u32 {
    4096
}
fn default_timeout() -> u32 {
    120
}

// ── Skill config ──────────────────────────────────────────────────

/// A reusable prompt fragment / skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillConfig {
    /// Skill name.
    pub name: String,

    /// Prompt text (inline string or file path reference).
    pub prompt: String,
}

// ── Helpers ───────────────────────────────────────────────────────

impl ProviderConfig {
    /// Returns the model identifier regardless of provider.
    pub fn model(&self) -> &str {
        match self {
            Self::Anthropic(c) => &c.model,
            Self::OpenAi(c) => &c.model,
            Self::Gemini(c) => &c.model,
            Self::Ollama(c) => &c.model,
        }
    }

    /// Returns the provider type as a string.
    pub fn provider_type(&self) -> &'static str {
        match self {
            Self::Anthropic(_) => "anthropic",
            Self::OpenAi(_) => "openai",
            Self::Gemini(_) => "gemini",
            Self::Ollama(_) => "ollama",
        }
    }

    /// Returns the API key env var name, if applicable.
    pub fn api_key_env(&self) -> Option<&str> {
        match self {
            Self::Anthropic(c) => c.api_key_env.as_deref(),
            Self::OpenAi(c) => c.api_key_env.as_deref(),
            Self::Gemini(c) => c.api_key_env.as_deref(),
            Self::Ollama(_) => None,
        }
    }

    /// Returns the base URL override, if any.
    pub fn base_url(&self) -> Option<&str> {
        match self {
            Self::Anthropic(c) => c.base_url.as_deref(),
            Self::OpenAi(c) => c.base_url.as_deref(),
            Self::Gemini(c) => c.base_url.as_deref(),
            Self::Ollama(c) => c.base_url.as_deref(),
        }
    }
}

/// Load all YAML agent configs from a directory.
pub fn load_yaml_agents(
    dir: &Path,
) -> Result<Vec<AgentYamlConfig>, YamlAgentError> {
    let mut agents = Vec::new();

    if !dir.exists() {
        return Err(YamlAgentError::DirNotFound(dir.to_path_buf()));
    }

    let entries = std::fs::read_dir(dir)
        .map_err(|e| YamlAgentError::Io(dir.to_path_buf(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());
        if !matches!(ext, Some("yaml" | "yml")) {
            continue;
        }

        let content = std::fs::read_to_string(&path)
            .map_err(|e| YamlAgentError::Io(path.clone(), e))?;

        // Support multi-document YAML (multiple agents in one file)
        for doc in serde_yaml::Deserializer::from_str(&content) {
            let config = AgentYamlConfig::deserialize(doc)
                .map_err(|e| YamlAgentError::Parse(path.clone(), e))?;
            agents.push(config);
        }
    }

    Ok(agents)
}

/// Errors from YAML agent loading.
#[derive(Debug, thiserror::Error)]
pub enum YamlAgentError {
    #[error("agents directory not found: {0}")]
    DirNotFound(std::path::PathBuf),
    #[error("I/O error reading {0}: {1}")]
    Io(std::path::PathBuf, std::io::Error),
    #[error("YAML parse error in {0}: {1}")]
    Parse(std::path::PathBuf, serde_yaml::Error),
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_anthropic_config() {
        let yaml = r#"
name: test-agent
description: A test agent
tier: Specialist
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
  api_key_env: ANTHROPIC_API_KEY
  anthropic:
    version: "2023-06-01"
execution:
  temperature: 0.2
  max_tokens: 2048
system_prompt: "You are a test agent."
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "test-agent");
        assert!(matches!(config.tier, AgentTier::Specialist));
        assert!(matches!(config.provider, ProviderConfig::Anthropic(_)));
        assert_eq!(config.provider.model(), "claude-sonnet-4-5-20250929");
        assert_eq!(config.provider.provider_type(), "anthropic");
        assert_eq!(config.execution.temperature, 0.2);
        assert_eq!(config.execution.max_tokens, 2048);
    }

    #[test]
    fn test_openai_config() {
        let yaml = r#"
name: openai-agent
provider:
  type: openai
  model: gpt-4o
  api_key_env: OPENAI_API_KEY
  base_url: "https://api.openai.com"
  openai:
    organization: org-123
    response_format: json_object
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "openai-agent");
        assert!(matches!(config.provider, ProviderConfig::OpenAi(_)));
        assert_eq!(config.provider.model(), "gpt-4o");
        if let ProviderConfig::OpenAi(ref c) = config.provider {
            assert_eq!(c.openai.organization.as_deref(), Some("org-123"));
            assert_eq!(c.openai.response_format.as_deref(), Some("json_object"));
        }
        // defaults
        assert_eq!(config.execution.temperature, 0.1);
        assert_eq!(config.execution.max_tokens, 4096);
    }

    #[test]
    fn test_gemini_config() {
        let yaml = r#"
name: gemini-agent
tier: Specialist
provider:
  type: gemini
  model: gemini-2.0-flash
  api_key_env: GEMINI_API_KEY
  gemini:
    safety_settings:
      harassment: block_only_high
    generation_config:
      candidate_count: 1
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "gemini-agent");
        assert!(matches!(config.provider, ProviderConfig::Gemini(_)));
        if let ProviderConfig::Gemini(ref c) = config.provider {
            assert_eq!(
                c.gemini.safety_settings.get("harassment").map(|s| s.as_str()),
                Some("block_only_high")
            );
        }
    }

    #[test]
    fn test_ollama_config() {
        let yaml = r#"
name: local-agent
provider:
  type: ollama
  model: llama3.1:70b
  base_url: "http://localhost:11434"
  ollama:
    num_ctx: 8192
    num_gpu: 1
execution:
  timeout_seconds: 300
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "local-agent");
        assert!(matches!(config.provider, ProviderConfig::Ollama(_)));
        assert_eq!(config.provider.api_key_env(), None);
        if let ProviderConfig::Ollama(ref c) = config.provider {
            assert_eq!(c.ollama.num_ctx, Some(8192));
            assert_eq!(c.ollama.num_gpu, Some(1));
        }
        assert_eq!(config.execution.timeout_seconds, 300);
    }

    #[test]
    fn test_skills_parsing() {
        let yaml = r#"
name: skilled-agent
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
skills:
  - name: summarize
    prompt: "Summarize the following text concisely."
  - name: translate
    prompt: "Translate the following to German."
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.skills.len(), 2);
        assert_eq!(config.skills[0].name, "summarize");
        assert_eq!(config.skills[1].name, "translate");
    }

    #[test]
    fn test_minimal_config() {
        let yaml = r#"
name: minimal
provider:
  type: ollama
  model: llama3.1
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "minimal");
        assert_eq!(config.description, "");
        assert!(matches!(config.tier, AgentTier::Specialist));
        assert!(config.tags.is_empty());
        assert!(config.group.is_none());
        assert!(config.skills.is_empty());
        assert_eq!(config.execution.temperature, 0.1);
        assert_eq!(config.execution.max_tokens, 4096);
        assert_eq!(config.execution.timeout_seconds, 120);
    }

    #[test]
    fn test_multi_document_yaml() {
        let yaml = r#"
---
name: agent-one
provider:
  type: ollama
  model: llama3.1
---
name: agent-two
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
"#;
        let mut agents = Vec::new();
        for doc in serde_yaml::Deserializer::from_str(yaml) {
            let config: AgentYamlConfig = AgentYamlConfig::deserialize(doc).unwrap();
            agents.push(config);
        }
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0].name, "agent-one");
        assert_eq!(agents[1].name, "agent-two");
    }

    #[test]
    fn test_provider_helpers() {
        let yaml = r#"
name: helper-test
provider:
  type: openai
  model: gpt-4o
  api_key_env: MY_KEY
  base_url: "https://custom.endpoint.com"
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.provider.model(), "gpt-4o");
        assert_eq!(config.provider.provider_type(), "openai");
        assert_eq!(config.provider.api_key_env(), Some("MY_KEY"));
        assert_eq!(config.provider.base_url(), Some("https://custom.endpoint.com"));
    }
}
