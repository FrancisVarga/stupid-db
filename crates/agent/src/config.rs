use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::info;

use crate::types::{AgentInfo, AgentTier};
use crate::yaml_schema::AgentYamlConfig;

/// Agent configuration loaded from a .md or .yaml file.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub tier: AgentTier,
    pub system_prompt: String,
}

impl From<AgentYamlConfig> for AgentConfig {
    fn from(yaml: AgentYamlConfig) -> Self {
        Self {
            name: yaml.name,
            description: yaml.description,
            tier: yaml.tier,
            system_prompt: yaml.system_prompt,
        }
    }
}

/// Load all agent configs from a directory of .md and .yaml files.
pub fn load_agents(agents_dir: &Path) -> Result<HashMap<String, AgentConfig>, AgentConfigError> {
    let mut agents = HashMap::new();

    if !agents_dir.exists() {
        return Err(AgentConfigError::DirNotFound(agents_dir.to_path_buf()));
    }

    let entries = std::fs::read_dir(agents_dir)
        .map_err(|e| AgentConfigError::IoError(agents_dir.to_path_buf(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());

        match ext {
            Some("md") => match load_agent_file(&path) {
                Ok(config) => {
                    info!(agent = %config.name, tier = ?config.tier, "loaded agent config");
                    agents.insert(config.name.clone(), config);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping agent file");
                }
            },
            Some("yaml" | "yml") => match load_yaml_agent_file(&path) {
                Ok(configs) => {
                    for config in configs {
                        info!(agent = %config.name, tier = ?config.tier, "loaded YAML agent config");
                        agents.insert(config.name.clone(), config);
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping YAML agent file");
                }
            },
            _ => {}
        }
    }

    Ok(agents)
}

/// Load agent configs from a single .yaml file (supports multi-document YAML).
fn load_yaml_agent_file(path: &Path) -> Result<Vec<AgentConfig>, AgentConfigError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AgentConfigError::IoError(path.to_path_buf(), e))?;

    let mut agents = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(&content) {
        let yaml_config = AgentYamlConfig::deserialize(doc)
            .map_err(|e| AgentConfigError::YamlError(path.to_path_buf(), e))?;
        agents.push(AgentConfig::from(yaml_config));
    }

    Ok(agents)
}

/// Load a single agent config from a .md file.
fn load_agent_file(path: &Path) -> Result<AgentConfig, AgentConfigError> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| AgentConfigError::IoError(path.to_path_buf(), e))?;

    // Parse YAML frontmatter between --- delimiters
    let (name, description, tier) = parse_frontmatter(&content, path)?;

    // Everything after the frontmatter is the system prompt
    let system_prompt = extract_body(&content);

    Ok(AgentConfig {
        name,
        description,
        tier,
        system_prompt,
    })
}

/// Parse YAML frontmatter from markdown content.
fn parse_frontmatter(
    content: &str,
    path: &Path,
) -> Result<(String, String, AgentTier), AgentConfigError> {
    let trimmed = content.trim_start();

    if !trimmed.starts_with("---") {
        return Err(AgentConfigError::NoFrontmatter(path.to_path_buf()));
    }

    let after_first = &trimmed[3..];
    let end = after_first
        .find("---")
        .ok_or_else(|| AgentConfigError::NoFrontmatter(path.to_path_buf()))?;

    let frontmatter = &after_first[..end];

    // Simple line-by-line YAML parsing (avoids full YAML dependency)
    let mut name = None;
    let mut description = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if let Some(val) = line.strip_prefix("name:") {
            name = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        } else if let Some(val) = line.strip_prefix("description:") {
            description = Some(val.trim().trim_matches('"').trim_matches('\'').to_string());
        }
    }

    let name = name.ok_or_else(|| AgentConfigError::MissingField(path.to_path_buf(), "name"))?;
    let description = description.unwrap_or_default();

    // Derive tier from agent name
    let tier = match name.as_str() {
        "architect" => AgentTier::Architect,
        n if n.ends_with("-lead") => AgentTier::Lead,
        _ => AgentTier::Specialist,
    };

    Ok((name, description, tier))
}

/// Extract the body content after the YAML frontmatter.
fn extract_body(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return content.to_string();
    }

    let after_first = &trimmed[3..];
    if let Some(end) = after_first.find("---") {
        after_first[end + 3..].trim().to_string()
    } else {
        content.to_string()
    }
}

/// Convert loaded agents to AgentInfo list.
pub fn agents_to_info(agents: &HashMap<String, AgentConfig>) -> Vec<AgentInfo> {
    let mut infos: Vec<_> = agents
        .values()
        .map(|a| AgentInfo {
            name: a.name.clone(),
            tier: a.tier,
            description: a.description.clone(),
        })
        .collect();
    infos.sort_by_key(|a| (a.tier as u8, a.name.clone()));
    infos
}

#[derive(Debug, thiserror::Error)]
pub enum AgentConfigError {
    #[error("agents directory not found: {0}")]
    DirNotFound(PathBuf),
    #[error("I/O error reading {0}: {1}")]
    IoError(PathBuf, std::io::Error),
    #[error("no YAML frontmatter in {0}")]
    NoFrontmatter(PathBuf),
    #[error("missing field '{1}' in {0}")]
    MissingField(PathBuf, &'static str),
    #[error("YAML parse error in {0}: {1}")]
    YamlError(PathBuf, serde_yaml::Error),
}

impl PartialOrd for AgentTier {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for AgentTier {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_yaml_agent_from_dir() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
name: test-yaml-agent
description: "Loaded from YAML"
tier: lead
provider:
  type: ollama
  model: llama3.1
system_prompt: "You are a test agent."
"#;
        std::fs::write(dir.path().join("test.yaml"), yaml).unwrap();

        let agents = load_agents(dir.path()).unwrap();
        assert_eq!(agents.len(), 1);
        let agent = agents.get("test-yaml-agent").unwrap();
        assert_eq!(agent.name, "test-yaml-agent");
        assert_eq!(agent.description, "Loaded from YAML");
        assert!(matches!(agent.tier, AgentTier::Lead));
        assert_eq!(agent.system_prompt, "You are a test agent.");
    }

    #[test]
    fn test_load_yml_extension() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"
name: yml-agent
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
"#;
        std::fs::write(dir.path().join("agent.yml"), yaml).unwrap();

        let agents = load_agents(dir.path()).unwrap();
        assert_eq!(agents.len(), 1);
        assert!(agents.contains_key("yml-agent"));
    }

    #[test]
    fn test_load_mixed_md_and_yaml() {
        let dir = tempfile::tempdir().unwrap();

        // .md agent
        let md = "---\nname: md-agent\ndescription: From markdown\n---\nYou are md.";
        std::fs::write(dir.path().join("md-agent.md"), md).unwrap();

        // .yaml agent
        let yaml = r#"
name: yaml-agent
description: "From YAML"
provider:
  type: ollama
  model: llama3.1
system_prompt: "You are yaml."
"#;
        std::fs::write(dir.path().join("yaml-agent.yaml"), yaml).unwrap();

        let agents = load_agents(dir.path()).unwrap();
        assert_eq!(agents.len(), 2);
        assert!(agents.contains_key("md-agent"));
        assert!(agents.contains_key("yaml-agent"));
    }

    #[test]
    fn test_multi_document_yaml_loading() {
        let dir = tempfile::tempdir().unwrap();
        let yaml = r#"---
name: agent-a
provider:
  type: ollama
  model: llama3.1
system_prompt: "Agent A"
---
name: agent-b
tier: architect
provider:
  type: anthropic
  model: claude-sonnet-4-5-20250929
system_prompt: "Agent B"
"#;
        std::fs::write(dir.path().join("multi.yaml"), yaml).unwrap();

        let agents = load_agents(dir.path()).unwrap();
        assert_eq!(agents.len(), 2);

        let a = agents.get("agent-a").unwrap();
        assert!(matches!(a.tier, AgentTier::Specialist)); // default
        assert_eq!(a.system_prompt, "Agent A");

        let b = agents.get("agent-b").unwrap();
        assert!(matches!(b.tier, AgentTier::Architect));
        assert_eq!(b.system_prompt, "Agent B");
    }

    #[test]
    fn test_yaml_overrides_md_same_name() {
        let dir = tempfile::tempdir().unwrap();

        // .md agent with name "shared"
        let md = "---\nname: shared\ndescription: From MD\n---\nMD prompt.";
        std::fs::write(dir.path().join("shared.md"), md).unwrap();

        // .yaml agent with same name — last writer wins (HashMap insert)
        let yaml = r#"
name: shared
description: "From YAML"
provider:
  type: ollama
  model: llama3.1
system_prompt: "YAML prompt."
"#;
        std::fs::write(dir.path().join("shared.yaml"), yaml).unwrap();

        let agents = load_agents(dir.path()).unwrap();
        // Both loaded, but only one survives in the HashMap
        assert_eq!(agents.len(), 1);
        assert!(agents.contains_key("shared"));
    }

    #[test]
    fn test_invalid_yaml_skipped() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("bad.yaml"), "not: valid: yaml: [").unwrap();

        // Should not error — invalid files are warned and skipped
        let agents = load_agents(dir.path()).unwrap();
        assert!(agents.is_empty());
    }

    #[test]
    fn test_from_yaml_config_conversion() {
        let yaml = r#"
name: convert-test
description: "Testing From impl"
tier: architect
provider:
  type: ollama
  model: llama3.1
system_prompt: "Hello from YAML."
skills:
  - name: skill1
    prompt: "Do something."
"#;
        let yaml_config: AgentYamlConfig = serde_yaml::from_str(yaml).unwrap();
        let config = AgentConfig::from(yaml_config);

        assert_eq!(config.name, "convert-test");
        assert_eq!(config.description, "Testing From impl");
        assert!(matches!(config.tier, AgentTier::Architect));
        assert_eq!(config.system_prompt, "Hello from YAML.");
    }
}
