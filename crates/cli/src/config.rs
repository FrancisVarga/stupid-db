use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use tracing::debug;

/// CLI configuration loaded from TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    /// Default LLM provider name (claude, openai, ollama)
    #[serde(default = "default_provider")]
    pub default_provider: String,

    /// Default model per provider
    #[serde(default)]
    pub default_models: HashMap<String, String>,

    /// API keys keyed by provider name
    #[serde(default)]
    pub api_keys: HashMap<String, String>,

    /// Tool permission overrides (tool_name -> "auto" | "confirm" | "deny")
    #[serde(default)]
    pub tool_permissions: HashMap<String, String>,

    /// Ollama base URL
    #[serde(default = "default_ollama_url")]
    pub ollama_url: String,

    /// OpenAI-compatible base URL
    #[serde(default = "default_openai_url")]
    pub openai_base_url: String,

    /// Maximum context window tokens
    #[serde(default = "default_max_tokens")]
    pub max_context_tokens: usize,

    /// Default server URL for remote mode
    #[serde(default)]
    pub server_url: Option<String>,
}

fn default_provider() -> String {
    "claude".to_string()
}

fn default_ollama_url() -> String {
    "http://localhost:11434".to_string()
}

fn default_openai_url() -> String {
    "https://api.openai.com".to_string()
}

fn default_max_tokens() -> usize {
    100_000
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            default_provider: default_provider(),
            default_models: HashMap::new(),
            api_keys: HashMap::new(),
            tool_permissions: HashMap::new(),
            ollama_url: default_ollama_url(),
            openai_base_url: default_openai_url(),
            max_context_tokens: default_max_tokens(),
            server_url: None,
        }
    }
}

impl CliConfig {
    /// Return the default config directory path: ~/.config/stupid-cli/
    pub fn default_config_dir() -> Result<PathBuf> {
        let config_dir = dirs::config_dir()
            .context("could not determine user config directory")?
            .join("stupid-cli");
        Ok(config_dir)
    }

    /// Return the default config file path.
    pub fn default_config_path() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join("config.toml"))
    }

    /// Load config from the given path, or the default path.
    /// Returns default config if the file does not exist.
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = match path {
            Some(p) => PathBuf::from(p),
            None => Self::default_config_path()?,
        };

        if config_path.exists() {
            debug!(?config_path, "Loading config");
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("failed to read config: {}", config_path.display()))?;
            let config: Self = toml::from_str(&content)
                .with_context(|| format!("failed to parse config: {}", config_path.display()))?;
            Ok(config)
        } else {
            debug!(?config_path, "Config file not found, using defaults");
            let config = Self::default();
            // Create directory and write default config
            if let Some(parent) = config_path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let toml_str = toml::to_string_pretty(&config)
                .context("failed to serialize default config")?;
            std::fs::write(&config_path, toml_str).ok();
            Ok(config)
        }
    }

    /// Resolve an API key for the given provider.
    /// Priority: cli_override > env var > config file.
    pub fn resolve_api_key(&self, provider: &str, cli_override: Option<&str>) -> Option<String> {
        // 1. CLI argument
        if let Some(key) = cli_override {
            return Some(key.to_string());
        }

        // 2. Environment variable
        let env_var = match provider {
            "claude" | "anthropic" => "ANTHROPIC_API_KEY",
            "openai" => "OPENAI_API_KEY",
            _ => return self.api_keys.get(provider).cloned(),
        };
        if let Ok(key) = std::env::var(env_var) {
            if !key.is_empty() {
                return Some(key);
            }
        }

        // 3. Config file
        self.api_keys.get(provider).cloned()
    }

    /// Resolve the model name for a provider.
    /// Priority: cli_override > config file > provider default.
    pub fn resolve_model(&self, provider: &str, cli_override: Option<&str>) -> String {
        if let Some(model) = cli_override {
            return model.to_string();
        }
        if let Some(model) = self.default_models.get(provider) {
            return model.clone();
        }
        // Provider defaults
        match provider {
            "claude" | "anthropic" => "claude-sonnet-4-20250514".to_string(),
            "openai" => "gpt-4o".to_string(),
            "ollama" => "llama3.2".to_string(),
            _ => "default".to_string(),
        }
    }

    /// Return the sessions directory path.
    pub fn sessions_dir() -> Result<PathBuf> {
        Ok(Self::default_config_dir()?.join("sessions"))
    }

    /// Ensure the sessions directory exists.
    pub fn ensure_sessions_dir() -> Result<PathBuf> {
        let dir = Self::sessions_dir()?;
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create sessions dir: {}", dir.display()))?;
        Ok(dir)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = CliConfig::default();
        assert_eq!(config.default_provider, "claude");
        assert_eq!(config.max_context_tokens, 100_000);
    }

    #[test]
    fn test_resolve_model_defaults() {
        let config = CliConfig::default();
        assert!(config.resolve_model("claude", None).contains("claude"));
        assert!(config.resolve_model("openai", None).contains("gpt"));
        assert!(config.resolve_model("ollama", None).contains("llama"));
    }

    #[test]
    fn test_resolve_model_override() {
        let config = CliConfig::default();
        assert_eq!(
            config.resolve_model("claude", Some("claude-opus-4-20250514")),
            "claude-opus-4-20250514"
        );
    }

    #[test]
    fn test_resolve_api_key_from_config() {
        // Use a custom provider name that has no env var mapping,
        // so the config file value is used.
        let mut config = CliConfig::default();
        config
            .api_keys
            .insert("custom-provider".to_string(), "sk-test-123".to_string());
        assert_eq!(
            config.resolve_api_key("custom-provider", None),
            Some("sk-test-123".to_string())
        );
    }

    #[test]
    fn test_resolve_api_key_cli_override() {
        let config = CliConfig::default();
        assert_eq!(
            config.resolve_api_key("claude", Some("cli-key")),
            Some("cli-key".to_string())
        );
    }

    #[test]
    fn test_toml_roundtrip() {
        let config = CliConfig::default();
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let parsed: CliConfig = toml::from_str(&toml_str).unwrap();
        assert_eq!(parsed.default_provider, config.default_provider);
    }
}
