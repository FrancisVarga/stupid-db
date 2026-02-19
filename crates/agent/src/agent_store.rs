//! Mutable agent store with CRUD operations and file write-back.
//!
//! Stores agents as `AgentYamlConfig` in memory with `Arc<RwLock<_>>` for
//! concurrent access. Mutations are persisted to individual `.yaml` files
//! in the agents directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::info;

use crate::config::AgentConfig;
use crate::types::AgentTier;
use crate::yaml_schema::AgentYamlConfig;

/// Mutable agent store with CRUD and file write-back.
pub struct AgentStore {
    dir: PathBuf,
    agents: Arc<RwLock<HashMap<String, AgentYamlConfig>>>,
}

impl AgentStore {
    /// Load all agents from directory (.md and .yaml/.yml files).
    pub fn new(agents_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(agents_dir)
            .with_context(|| format!("failed to create agents dir: {}", agents_dir.display()))?;

        let agents = load_all(agents_dir)?;
        let count = agents.len();
        info!(path = %agents_dir.display(), count, "agent store initialized");

        Ok(Self {
            dir: agents_dir.to_path_buf(),
            agents: Arc::new(RwLock::new(agents)),
        })
    }

    /// List all agents.
    pub async fn list(&self) -> Vec<AgentYamlConfig> {
        let map = self.agents.read().await;
        map.values().cloned().collect()
    }

    /// Get a single agent by name.
    pub async fn get(&self, name: &str) -> Option<AgentYamlConfig> {
        let map = self.agents.read().await;
        map.get(name).cloned()
    }

    /// Create a new agent (writes .yaml file to disk).
    pub async fn create(&self, config: AgentYamlConfig) -> Result<AgentYamlConfig> {
        let mut map = self.agents.write().await;
        if map.contains_key(&config.name) {
            bail!("agent already exists: {}", config.name);
        }
        self.write_yaml(&config)?;
        info!(agent = %config.name, "agent created");
        map.insert(config.name.clone(), config.clone());
        Ok(config)
    }

    /// Update an existing agent. Returns `None` if agent not found.
    pub async fn update(
        &self,
        name: &str,
        config: AgentYamlConfig,
    ) -> Result<Option<AgentYamlConfig>> {
        let mut map = self.agents.write().await;
        if !map.contains_key(name) {
            return Ok(None);
        }

        // If the name changed, remove old file and old map entry
        if config.name != name {
            if map.contains_key(&config.name) {
                bail!(
                    "cannot rename '{}' to '{}': target name already exists",
                    name,
                    config.name
                );
            }
            let old_path = self.agent_file_path(name);
            if old_path.exists() {
                std::fs::remove_file(&old_path)
                    .with_context(|| format!("failed to remove old agent file: {}", old_path.display()))?;
            }
            map.remove(name);
        }

        self.write_yaml(&config)?;
        info!(agent = %config.name, "agent updated");
        map.insert(config.name.clone(), config.clone());
        Ok(Some(config))
    }

    /// Delete an agent (removes file from disk). Returns `true` if it existed.
    pub async fn delete(&self, name: &str) -> Result<bool> {
        let mut map = self.agents.write().await;
        if map.remove(name).is_none() {
            return Ok(false);
        }

        let path = self.agent_file_path(name);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete agent file: {}", path.display()))?;
        }
        info!(agent = %name, "agent deleted");
        Ok(true)
    }

    /// Hot-reload: re-scan directory and refresh in-memory state.
    /// Returns the number of agents loaded.
    pub async fn reload(&self) -> Result<usize> {
        let fresh = load_all(&self.dir)?;
        let count = fresh.len();
        let mut map = self.agents.write().await;
        *map = fresh;
        info!(count, "agent store reloaded");
        Ok(count)
    }

    /// Get `AgentConfig` for executor compatibility.
    pub async fn get_agent_config(&self, name: &str) -> Option<AgentConfig> {
        self.get(name).await.map(AgentConfig::from)
    }

    /// Get all agents as `AgentConfig` HashMap (for executor compatibility).
    pub async fn to_agent_configs(&self) -> HashMap<String, AgentConfig> {
        let map = self.agents.read().await;
        map.iter()
            .map(|(k, v)| (k.clone(), AgentConfig::from(v.clone())))
            .collect()
    }

    fn agent_file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yaml", name))
    }

    fn write_yaml(&self, config: &AgentYamlConfig) -> Result<()> {
        let path = self.agent_file_path(&config.name);
        let yaml = serde_yaml::to_string(config)
            .with_context(|| format!("failed to serialize agent: {}", config.name))?;
        std::fs::write(&path, yaml)
            .with_context(|| format!("failed to write agent file: {}", path.display()))?;
        Ok(())
    }
}

// ── Directory loading ─────────────────────────────────────────────

/// Scan a directory and load all .md and .yaml/.yml agents into a HashMap.
fn load_all(dir: &Path) -> Result<HashMap<String, AgentYamlConfig>> {
    let mut agents = HashMap::new();

    if !dir.exists() {
        return Ok(agents);
    }

    let entries =
        std::fs::read_dir(dir).with_context(|| format!("failed to read agents dir: {}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());

        match ext {
            Some("yaml" | "yml") => match load_yaml_file(&path) {
                Ok(configs) => {
                    for c in configs {
                        agents.insert(c.name.clone(), c);
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping YAML agent file");
                }
            },
            Some("md") => match load_md_as_yaml(&path) {
                Ok(config) => {
                    agents.insert(config.name.clone(), config);
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping .md agent file");
                }
            },
            _ => {}
        }
    }

    Ok(agents)
}

/// Load agents from a .yaml file (supports multi-document YAML).
fn load_yaml_file(path: &Path) -> Result<Vec<AgentYamlConfig>> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read: {}", path.display()))?;

    let mut configs = Vec::new();
    for doc in serde_yaml::Deserializer::from_str(&content) {
        let config = AgentYamlConfig::deserialize(doc)
            .with_context(|| format!("YAML parse error in {}", path.display()))?;
        configs.push(config);
    }
    Ok(configs)
}

/// Convert a .md frontmatter agent into an `AgentYamlConfig` for uniform storage.
fn load_md_as_yaml(path: &Path) -> Result<AgentYamlConfig> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read: {}", path.display()))?;

    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        bail!("no YAML frontmatter in {}", path.display());
    }

    let after_first = &trimmed[3..];
    let end = after_first
        .find("---")
        .ok_or_else(|| anyhow::anyhow!("no closing --- in {}", path.display()))?;

    let frontmatter = &after_first[..end];
    let body = after_first[end + 3..].trim().to_string();

    // Parse frontmatter fields
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

    let name = name.ok_or_else(|| anyhow::anyhow!("missing 'name' in {}", path.display()))?;

    let tier = match name.as_str() {
        "architect" => AgentTier::Architect,
        n if n.ends_with("-lead") => AgentTier::Lead,
        _ => AgentTier::Specialist,
    };

    Ok(AgentYamlConfig {
        name,
        description: description.unwrap_or_default(),
        tier,
        tags: Vec::new(),
        group: None,
        provider: crate::yaml_schema::ProviderConfig::Ollama(crate::yaml_schema::OllamaConfig {
            model: "default".to_string(),
            base_url: None,
            ollama: Default::default(),
        }),
        execution: Default::default(),
        system_prompt: body,
        skills: Vec::new(),
        skill_refs: Vec::new(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::yaml_schema::{OllamaConfig, OllamaSpecific, ProviderConfig};
    use tempfile::TempDir;

    fn make_config(name: &str) -> AgentYamlConfig {
        AgentYamlConfig {
            name: name.to_string(),
            description: format!("{} description", name),
            tier: AgentTier::Specialist,
            tags: vec!["test".to_string()],
            group: None,
            provider: ProviderConfig::Ollama(OllamaConfig {
                model: "llama3.1".to_string(),
                base_url: None,
                ollama: OllamaSpecific::default(),
            }),
            execution: Default::default(),
            system_prompt: format!("You are {}.", name),
            skills: Vec::new(),
            skill_refs: Vec::new(),
        }
    }

    fn setup() -> (TempDir, AgentStore) {
        let tmp = TempDir::new().unwrap();
        let store = AgentStore::new(tmp.path()).unwrap();
        (tmp, store)
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let (_tmp, store) = setup();
        assert!(store.list().await.is_empty());

        let config = make_config("analyzer");
        let created = store.create(config).await.unwrap();
        assert_eq!(created.name, "analyzer");

        let fetched = store.get("analyzer").await.unwrap();
        assert_eq!(fetched.name, "analyzer");
        assert_eq!(fetched.description, "analyzer description");
        assert_eq!(fetched.system_prompt, "You are analyzer.");
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let (_tmp, store) = setup();
        store.create(make_config("agent-a")).await.unwrap();
        let err = store.create(make_config("agent-a")).await.unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_list() {
        let (_tmp, store) = setup();
        store.create(make_config("alpha")).await.unwrap();
        store.create(make_config("beta")).await.unwrap();

        let list = store.list().await;
        assert_eq!(list.len(), 2);
        let names: Vec<_> = list.iter().map(|a| a.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[tokio::test]
    async fn test_update() {
        let (_tmp, store) = setup();
        store.create(make_config("updatable")).await.unwrap();

        let mut updated = make_config("updatable");
        updated.description = "new description".to_string();
        updated.system_prompt = "Updated prompt.".to_string();

        let result = store.update("updatable", updated).await.unwrap().unwrap();
        assert_eq!(result.description, "new description");

        let fetched = store.get("updatable").await.unwrap();
        assert_eq!(fetched.system_prompt, "Updated prompt.");
    }

    #[tokio::test]
    async fn test_update_not_found() {
        let (_tmp, store) = setup();
        let result = store.update("ghost", make_config("ghost")).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let (tmp, store) = setup();
        store.create(make_config("doomed")).await.unwrap();

        // File exists on disk
        assert!(tmp.path().join("doomed.yaml").exists());

        assert!(store.delete("doomed").await.unwrap());
        assert!(store.get("doomed").await.is_none());

        // File removed from disk
        assert!(!tmp.path().join("doomed.yaml").exists());
    }

    #[tokio::test]
    async fn test_delete_not_found() {
        let (_tmp, store) = setup();
        assert!(!store.delete("ghost").await.unwrap());
    }

    #[tokio::test]
    async fn test_reload_picks_up_new_files() {
        let (tmp, store) = setup();
        assert!(store.list().await.is_empty());

        // Write a YAML file directly to disk (simulating external edit)
        let yaml = r#"
name: external-agent
provider:
  type: ollama
  model: llama3.1
system_prompt: "I was added externally."
"#;
        std::fs::write(tmp.path().join("external-agent.yaml"), yaml).unwrap();

        let count = store.reload().await.unwrap();
        assert_eq!(count, 1);

        let agent = store.get("external-agent").await.unwrap();
        assert_eq!(agent.system_prompt, "I was added externally.");
    }

    #[tokio::test]
    async fn test_reload_removes_deleted_files() {
        let (tmp, store) = setup();
        store.create(make_config("ephemeral")).await.unwrap();
        assert_eq!(store.list().await.len(), 1);

        // Delete file externally
        std::fs::remove_file(tmp.path().join("ephemeral.yaml")).unwrap();

        let count = store.reload().await.unwrap();
        assert_eq!(count, 0);
        assert!(store.get("ephemeral").await.is_none());
    }

    #[tokio::test]
    async fn test_write_back_produces_valid_yaml() {
        let (tmp, store) = setup();
        store.create(make_config("roundtrip")).await.unwrap();

        // Read the written file and parse it back
        let path = tmp.path().join("roundtrip.yaml");
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: AgentYamlConfig = serde_yaml::from_str(&content).unwrap();
        assert_eq!(parsed.name, "roundtrip");
        assert_eq!(parsed.system_prompt, "You are roundtrip.");
    }

    #[tokio::test]
    async fn test_to_agent_configs() {
        let (_tmp, store) = setup();
        store.create(make_config("compat")).await.unwrap();

        let configs = store.to_agent_configs().await;
        assert_eq!(configs.len(), 1);
        let config = configs.get("compat").unwrap();
        assert_eq!(config.name, "compat");
        assert!(matches!(config.tier, AgentTier::Specialist));
    }

    #[tokio::test]
    async fn test_get_agent_config() {
        let (_tmp, store) = setup();
        store.create(make_config("lookup")).await.unwrap();

        let config = store.get_agent_config("lookup").await.unwrap();
        assert_eq!(config.name, "lookup");
        assert_eq!(config.system_prompt, "You are lookup.");
    }

    #[tokio::test]
    async fn test_loads_md_files() {
        let (tmp, store) = setup();
        let md = "---\nname: md-bot\ndescription: From markdown\n---\nYou are md-bot.";
        std::fs::write(tmp.path().join("md-bot.md"), md).unwrap();

        store.reload().await.unwrap();

        let agent = store.get("md-bot").await.unwrap();
        assert_eq!(agent.name, "md-bot");
        assert_eq!(agent.description, "From markdown");
        assert_eq!(agent.system_prompt, "You are md-bot.");
    }

    #[tokio::test]
    async fn test_update_with_rename() {
        let (tmp, store) = setup();
        store.create(make_config("old-name")).await.unwrap();
        assert!(tmp.path().join("old-name.yaml").exists());

        let mut renamed = make_config("new-name");
        renamed.description = "renamed agent".to_string();

        let result = store.update("old-name", renamed).await.unwrap().unwrap();
        assert_eq!(result.name, "new-name");

        // Old file removed, new file exists
        assert!(!tmp.path().join("old-name.yaml").exists());
        assert!(tmp.path().join("new-name.yaml").exists());

        // Old name gone, new name present
        assert!(store.get("old-name").await.is_none());
        assert!(store.get("new-name").await.is_some());
    }
}
