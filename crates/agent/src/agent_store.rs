//! Mutable agent store with CRUD operations and file write-back.
//!
//! Stores agents as `AgentEntry` (config + source file path) in memory with
//! `Arc<RwLock<_>>` for concurrent access. Mutations are persisted to individual
//! `.yaml` files. The directory is scanned recursively via `walkdir`, so agents
//! in subdirectories are discovered automatically. Each agent tracks its
//! original file path so write-back targets the correct location.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use tokio::sync::RwLock;
use tracing::info;
use walkdir::WalkDir;

use crate::config::AgentConfig;
use crate::types::AgentTier;
use crate::yaml_schema::AgentYamlConfig;

/// An agent config paired with its source file path for correct write-back.
#[derive(Debug, Clone)]
struct AgentEntry {
    config: AgentYamlConfig,
    /// Absolute path to the file this agent was loaded from (or will be written to).
    file_path: PathBuf,
}

/// Mutable agent store with CRUD and file write-back.
pub struct AgentStore {
    dir: PathBuf,
    agents: Arc<RwLock<HashMap<String, AgentEntry>>>,
}

impl AgentStore {
    /// Load all agents from directory recursively (.md and .yaml/.yml files).
    pub fn new(agents_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(agents_dir)
            .with_context(|| format!("failed to create agents dir: {}", agents_dir.display()))?;

        let agents = load_all(agents_dir)?;
        let count = agents.len();
        let names: Vec<_> = agents.keys().collect();
        info!(path = %agents_dir.display(), count, ?names, "agent store initialized");

        Ok(Self {
            dir: agents_dir.to_path_buf(),
            agents: Arc::new(RwLock::new(agents)),
        })
    }

    /// List all agents.
    pub async fn list(&self) -> Vec<AgentYamlConfig> {
        let map = self.agents.read().await;
        map.values().map(|e| e.config.clone()).collect()
    }

    /// Get a single agent by name.
    pub async fn get(&self, name: &str) -> Option<AgentYamlConfig> {
        let map = self.agents.read().await;
        let result = map.get(name).map(|e| e.config.clone());
        if result.is_none() {
            let available: Vec<_> = map.keys().collect();
            tracing::debug!(requested = name, ?available, "agent store lookup miss");
        }
        result
    }

    /// Create a new agent (writes .yaml file to root agents dir).
    pub async fn create(&self, config: AgentYamlConfig) -> Result<AgentYamlConfig> {
        let mut map = self.agents.write().await;
        if map.contains_key(&config.name) {
            bail!("agent already exists: {}", config.name);
        }
        let file_path = self.default_file_path(&config.name);
        write_yaml_to(&file_path, &config)?;
        info!(agent = %config.name, path = %file_path.display(), "agent created");
        map.insert(
            config.name.clone(),
            AgentEntry {
                config: config.clone(),
                file_path,
            },
        );
        Ok(config)
    }

    /// Update an existing agent. Returns `None` if agent not found.
    pub async fn update(
        &self,
        name: &str,
        config: AgentYamlConfig,
    ) -> Result<Option<AgentYamlConfig>> {
        let mut map = self.agents.write().await;
        let existing = match map.get(name) {
            Some(e) => e.clone(),
            None => return Ok(None),
        };

        // If the name changed, remove old file and old map entry
        if config.name != name {
            if map.contains_key(&config.name) {
                bail!(
                    "cannot rename '{}' to '{}': target name already exists",
                    name,
                    config.name
                );
            }
            if existing.file_path.exists() {
                std::fs::remove_file(&existing.file_path).with_context(|| {
                    format!(
                        "failed to remove old agent file: {}",
                        existing.file_path.display()
                    )
                })?;
            }
            map.remove(name);

            // Renamed agents get a new file in the same directory as the original
            let new_path = existing
                .file_path
                .parent()
                .unwrap_or(&self.dir)
                .join(format!("{}.yaml", config.name));
            write_yaml_to(&new_path, &config)?;
            info!(agent = %config.name, path = %new_path.display(), "agent updated (renamed)");
            map.insert(
                config.name.clone(),
                AgentEntry {
                    config: config.clone(),
                    file_path: new_path,
                },
            );
        } else {
            // Write back to the tracked path
            write_yaml_to(&existing.file_path, &config)?;
            info!(agent = %config.name, path = %existing.file_path.display(), "agent updated");
            map.insert(
                config.name.clone(),
                AgentEntry {
                    config: config.clone(),
                    file_path: existing.file_path,
                },
            );
        }

        Ok(Some(config))
    }

    /// Delete an agent (removes file from disk). Returns `true` if it existed.
    pub async fn delete(&self, name: &str) -> Result<bool> {
        let mut map = self.agents.write().await;
        let entry = match map.remove(name) {
            Some(e) => e,
            None => return Ok(false),
        };

        if entry.file_path.exists() {
            std::fs::remove_file(&entry.file_path).with_context(|| {
                format!(
                    "failed to delete agent file: {}",
                    entry.file_path.display()
                )
            })?;
        }
        info!(agent = %name, path = %entry.file_path.display(), "agent deleted");
        Ok(true)
    }

    /// Hot-reload: re-scan directory recursively and refresh in-memory state.
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
            .map(|(k, e)| (k.clone(), AgentConfig::from(e.config.clone())))
            .collect()
    }

    /// Default file path for newly created agents (root agents dir).
    fn default_file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yaml", name))
    }
}

/// Serialize an `AgentYamlConfig` to a YAML file at `path`.
fn write_yaml_to(path: &Path, config: &AgentYamlConfig) -> Result<()> {
    let yaml = serde_yaml::to_string(config)
        .with_context(|| format!("failed to serialize agent: {}", config.name))?;
    std::fs::write(path, yaml)
        .with_context(|| format!("failed to write agent file: {}", path.display()))?;
    Ok(())
}

// ── Directory loading ─────────────────────────────────────────────

/// Recursively scan a directory and load all .md and .yaml/.yml agents.
fn load_all(dir: &Path) -> Result<HashMap<String, AgentEntry>> {
    let mut agents = HashMap::new();

    if !dir.exists() {
        return Ok(agents);
    }

    for entry in WalkDir::new(dir).follow_links(true).into_iter() {
        let entry = match entry {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(error = %e, "walkdir error, skipping entry");
                continue;
            }
        };

        if !entry.file_type().is_file() {
            continue;
        }

        let path = entry.into_path();
        let ext = path.extension().and_then(|e| e.to_str());

        match ext {
            Some("yaml" | "yml") => match load_yaml_file(&path) {
                Ok(configs) => {
                    for c in configs {
                        agents.insert(
                            c.name.clone(),
                            AgentEntry {
                                config: c,
                                file_path: path.clone(),
                            },
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "skipping YAML agent file");
                }
            },
            Some("md") => match load_md_as_yaml(&path) {
                Ok(config) => {
                    let name = config.name.clone();
                    agents.insert(
                        name,
                        AgentEntry {
                            config,
                            file_path: path,
                        },
                    );
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

    // ── Recursive scan + write-back path tracking tests ───────────

    #[tokio::test]
    async fn test_loads_agents_from_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("team-alpha");
        std::fs::create_dir_all(&subdir).unwrap();

        // Agent in root
        let root_yaml = r#"
name: root-agent
provider:
  type: ollama
  model: llama3.1
system_prompt: "I live at root."
"#;
        std::fs::write(tmp.path().join("root-agent.yaml"), root_yaml).unwrap();

        // Agent in subdirectory
        let sub_yaml = r#"
name: sub-agent
provider:
  type: ollama
  model: llama3.1
system_prompt: "I live in a subdirectory."
"#;
        std::fs::write(subdir.join("sub-agent.yaml"), sub_yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();
        let list = store.list().await;
        assert_eq!(list.len(), 2);

        let root = store.get("root-agent").await.unwrap();
        assert_eq!(root.system_prompt, "I live at root.");

        let sub = store.get("sub-agent").await.unwrap();
        assert_eq!(sub.system_prompt, "I live in a subdirectory.");
    }

    #[tokio::test]
    async fn test_loads_agents_from_nested_subdirectories() {
        let tmp = TempDir::new().unwrap();
        let nested = tmp.path().join("level1").join("level2");
        std::fs::create_dir_all(&nested).unwrap();

        let yaml = r#"
name: deeply-nested
provider:
  type: ollama
  model: llama3.1
system_prompt: "I am deeply nested."
"#;
        std::fs::write(nested.join("deeply-nested.yaml"), yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();
        let agent = store.get("deeply-nested").await.unwrap();
        assert_eq!(agent.system_prompt, "I am deeply nested.");
    }

    #[tokio::test]
    async fn test_write_back_targets_original_subdirectory_path() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("custom-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let yaml = r#"
name: tracked-agent
provider:
  type: ollama
  model: llama3.1
system_prompt: "Original prompt."
"#;
        std::fs::write(subdir.join("tracked-agent.yaml"), yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();

        // Update the agent — should write back to subdir, not root
        let mut updated = store.get("tracked-agent").await.unwrap();
        updated.system_prompt = "Updated prompt.".to_string();
        store.update("tracked-agent", updated).await.unwrap();

        // Verify the file in subdir was updated
        let content = std::fs::read_to_string(subdir.join("tracked-agent.yaml")).unwrap();
        assert!(content.contains("Updated prompt."));

        // Verify NO file was created in root
        assert!(!tmp.path().join("tracked-agent.yaml").exists());
    }

    #[tokio::test]
    async fn test_delete_removes_file_from_subdirectory() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("deletable-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let yaml = r#"
name: to-delete
provider:
  type: ollama
  model: llama3.1
system_prompt: "Delete me."
"#;
        let file_path = subdir.join("to-delete.yaml");
        std::fs::write(&file_path, yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();
        assert!(store.get("to-delete").await.is_some());

        store.delete("to-delete").await.unwrap();
        assert!(store.get("to-delete").await.is_none());
        assert!(!file_path.exists());
    }

    #[tokio::test]
    async fn test_new_agent_created_in_root_dir() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("existing-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();

        // Create via API should go to root
        store.create(make_config("api-created")).await.unwrap();
        assert!(tmp.path().join("api-created.yaml").exists());
        assert!(!subdir.join("api-created.yaml").exists());
    }

    #[tokio::test]
    async fn test_rename_in_subdirectory_stays_in_subdirectory() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("rename-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let yaml = r#"
name: before-rename
provider:
  type: ollama
  model: llama3.1
system_prompt: "Will be renamed."
"#;
        std::fs::write(subdir.join("before-rename.yaml"), yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();

        let mut renamed = store.get("before-rename").await.unwrap();
        renamed.name = "after-rename".to_string();
        store.update("before-rename", renamed).await.unwrap();

        // Old file gone
        assert!(!subdir.join("before-rename.yaml").exists());
        // New file in same subdirectory, not root
        assert!(subdir.join("after-rename.yaml").exists());
        assert!(!tmp.path().join("after-rename.yaml").exists());

        // Store has correct state
        assert!(store.get("before-rename").await.is_none());
        assert!(store.get("after-rename").await.is_some());
    }

    #[tokio::test]
    async fn test_reload_preserves_subdirectory_agents() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("reload-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let yaml = r#"
name: reload-sub
provider:
  type: ollama
  model: llama3.1
system_prompt: "Survives reload."
"#;
        std::fs::write(subdir.join("reload-sub.yaml"), yaml).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();
        assert!(store.get("reload-sub").await.is_some());

        // Reload and verify still found
        store.reload().await.unwrap();
        let agent = store.get("reload-sub").await.unwrap();
        assert_eq!(agent.system_prompt, "Survives reload.");
    }

    #[tokio::test]
    async fn test_md_in_subdirectory() {
        let tmp = TempDir::new().unwrap();
        let subdir = tmp.path().join("md-team");
        std::fs::create_dir_all(&subdir).unwrap();

        let md = "---\nname: nested-md\ndescription: Nested markdown agent\n---\nYou are nested-md.";
        std::fs::write(subdir.join("nested-md.md"), md).unwrap();

        let store = AgentStore::new(tmp.path()).unwrap();
        let agent = store.get("nested-md").await.unwrap();
        assert_eq!(agent.name, "nested-md");
        assert_eq!(agent.description, "Nested markdown agent");
        assert_eq!(agent.system_prompt, "You are nested-md.");
    }
}
