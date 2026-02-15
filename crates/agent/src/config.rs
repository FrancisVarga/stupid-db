use std::collections::HashMap;
use std::path::{Path, PathBuf};

use tracing::info;

use crate::types::{AgentInfo, AgentTier};

/// Agent configuration loaded from a .md file with YAML frontmatter.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    pub name: String,
    pub description: String,
    pub tier: AgentTier,
    pub system_prompt: String,
}

/// Load all agent configs from a directory of .md files.
pub fn load_agents(agents_dir: &Path) -> Result<HashMap<String, AgentConfig>, AgentConfigError> {
    let mut agents = HashMap::new();

    if !agents_dir.exists() {
        return Err(AgentConfigError::DirNotFound(agents_dir.to_path_buf()));
    }

    let entries = std::fs::read_dir(agents_dir)
        .map_err(|e| AgentConfigError::IoError(agents_dir.to_path_buf(), e))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|e| e == "md") {
            match load_agent_file(&path) {
                Ok(config) => {
                    info!(agent = %config.name, tier = ?config.tier, "loaded agent config");
                    agents.insert(config.name.clone(), config);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping agent file");
                }
            }
        }
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
