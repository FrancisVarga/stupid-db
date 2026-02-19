//! YAML-based agent configuration schema.
//!
//! Supports 4 LLM providers: Anthropic, OpenAI, Gemini, and Ollama.
//! Agent configs live in `data/agents/*.yaml` (user-created) and
//! `data/bundeswehr/agents/**/*.yaml` (internal, seeded at startup).
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

    /// Reusable prompt fragments (embedded inline).
    #[serde(default)]
    pub skills: Vec<SkillConfig>,

    /// References to standalone skill files by name.
    #[serde(default)]
    pub skill_refs: Vec<String>,
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

// ── Standalone skill config ───────────────────────────────────────

/// A standalone skill definition loaded from `data/bundeswehr/skills/*.yml`.
///
/// Richer than embedded [`SkillConfig`]: includes description, tags, and
/// version for discovery and cross-agent reuse via `skill_refs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillYamlConfig {
    /// Unique skill name (used as the reference key in `skill_refs`).
    pub name: String,

    /// Human-readable description.
    #[serde(default)]
    pub description: String,

    /// The prompt text for this skill.
    pub prompt: String,

    /// Searchable tags.
    #[serde(default)]
    pub tags: Vec<String>,

    /// Semver version string.
    #[serde(default = "default_skill_version")]
    pub version: String,
}

fn default_skill_version() -> String {
    "1.0.0".to_string()
}

/// Load a single standalone skill from a YAML file.
pub fn load_skill(path: &Path) -> Result<SkillYamlConfig, YamlAgentError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| YamlAgentError::Io(path.to_path_buf(), e))?;
    let config: SkillYamlConfig = serde_yaml::from_str(&content)
        .map_err(|e| YamlAgentError::Parse(path.to_path_buf(), e))?;
    Ok(config)
}

/// Load all standalone skills from a directory and validate name uniqueness.
pub fn load_skills(
    dir: &Path,
) -> Result<Vec<SkillYamlConfig>, YamlAgentError> {
    let mut skills = Vec::new();

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
        skills.push(load_skill(&path)?);
    }

    // Validate name uniqueness.
    let mut seen = std::collections::HashSet::new();
    for skill in &skills {
        if !seen.insert(&skill.name) {
            return Err(YamlAgentError::DuplicateSkillName(skill.name.clone()));
        }
    }

    Ok(skills)
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
    #[error("duplicate skill name: {0}")]
    DuplicateSkillName(String),
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
tier: specialist
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
tier: specialist
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
    fn test_skill_refs_parsing() {
        let yaml = r#"
name: ref-agent
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
skill_refs:
  - summarize
  - translate
skills:
  - name: inline-skill
    prompt: "Do something inline."
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.skill_refs, vec!["summarize", "translate"]);
        assert_eq!(config.skills.len(), 1);
        assert_eq!(config.skills[0].name, "inline-skill");
    }

    #[test]
    fn test_skill_refs_default_empty() {
        let yaml = r#"
name: no-refs
provider:
  type: ollama
  model: llama3.1
"#;
        let config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert!(config.skill_refs.is_empty());
    }

    #[test]
    fn test_standalone_skill_parsing() {
        let yaml = r#"
name: summarize
description: "Summarize text concisely"
prompt: "Please summarize the following text."
tags:
  - nlp
  - text
version: "1.2.0"
"#;
        let config: SkillYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "summarize");
        assert_eq!(config.description, "Summarize text concisely");
        assert_eq!(config.prompt, "Please summarize the following text.");
        assert_eq!(config.tags, vec!["nlp", "text"]);
        assert_eq!(config.version, "1.2.0");
    }

    #[test]
    fn test_standalone_skill_defaults() {
        let yaml = r#"
name: minimal-skill
prompt: "Do the thing."
"#;
        let config: SkillYamlConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(config.name, "minimal-skill");
        assert_eq!(config.description, "");
        assert!(config.tags.is_empty());
        assert_eq!(config.version, "1.0.0");
    }

    #[test]
    fn test_load_skill_from_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("summarize.yml");
        std::fs::write(
            &path,
            r#"
name: summarize
description: Summarize text
prompt: "Summarize the following."
tags: [nlp]
"#,
        )
        .unwrap();

        let skill = load_skill(&path).unwrap();
        assert_eq!(skill.name, "summarize");
        assert_eq!(skill.tags, vec!["nlp"]);
        assert_eq!(skill.version, "1.0.0");
    }

    #[test]
    fn test_load_skills_directory() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.yml"),
            "name: alpha\nprompt: do alpha\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.yaml"),
            "name: beta\nprompt: do beta\n",
        )
        .unwrap();
        // Non-YAML file should be ignored.
        std::fs::write(dir.path().join("readme.txt"), "ignore me").unwrap();

        let skills = load_skills(dir.path()).unwrap();
        assert_eq!(skills.len(), 2);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[test]
    fn test_load_skills_duplicate_name_error() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("a.yml"),
            "name: same\nprompt: first\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("b.yml"),
            "name: same\nprompt: second\n",
        )
        .unwrap();

        let result = load_skills(dir.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("duplicate skill name: same"));
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
