//! Mutable skill store with CRUD operations and file write-back.
//!
//! Stores skills as `SkillYamlConfig` in memory with `Arc<RwLock<_>>` for
//! concurrent access. Mutations are persisted to individual `.yml` files
//! in the skills directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{bail, Context, Result};
use tokio::sync::RwLock;
use tracing::info;

use crate::yaml_schema::SkillYamlConfig;

/// Mutable skill store with CRUD and file write-back.
pub struct SkillStore {
    dir: PathBuf,
    skills: Arc<RwLock<HashMap<String, SkillYamlConfig>>>,
}

impl SkillStore {
    /// Load all skills from directory (.yaml/.yml files).
    pub fn new(skills_dir: &Path) -> Result<Self> {
        std::fs::create_dir_all(skills_dir)
            .with_context(|| format!("failed to create skills dir: {}", skills_dir.display()))?;

        let skills = load_all(skills_dir)?;
        let count = skills.len();
        info!(path = %skills_dir.display(), count, "skill store initialized");

        Ok(Self {
            dir: skills_dir.to_path_buf(),
            skills: Arc::new(RwLock::new(skills)),
        })
    }

    /// List all skills.
    pub async fn list(&self) -> Vec<SkillYamlConfig> {
        let map = self.skills.read().await;
        map.values().cloned().collect()
    }

    /// Get a single skill by name.
    pub async fn get(&self, name: &str) -> Option<SkillYamlConfig> {
        let map = self.skills.read().await;
        map.get(name).cloned()
    }

    /// Create a new skill (writes .yml file to disk).
    pub async fn create(&self, config: SkillYamlConfig) -> Result<SkillYamlConfig> {
        let mut map = self.skills.write().await;
        if map.contains_key(&config.name) {
            bail!("skill already exists: {}", config.name);
        }
        self.write_yaml(&config)?;
        info!(skill = %config.name, "skill created");
        map.insert(config.name.clone(), config.clone());
        Ok(config)
    }

    /// Update an existing skill. Returns `None` if skill not found.
    pub async fn update(
        &self,
        name: &str,
        config: SkillYamlConfig,
    ) -> Result<Option<SkillYamlConfig>> {
        let mut map = self.skills.write().await;
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
            let old_path = self.skill_file_path(name);
            if old_path.exists() {
                std::fs::remove_file(&old_path)
                    .with_context(|| format!("failed to remove old skill file: {}", old_path.display()))?;
            }
            map.remove(name);
        }

        self.write_yaml(&config)?;
        info!(skill = %config.name, "skill updated");
        map.insert(config.name.clone(), config.clone());
        Ok(Some(config))
    }

    /// Delete a skill (removes file from disk). Returns `true` if it existed.
    pub async fn delete(&self, name: &str) -> Result<bool> {
        let mut map = self.skills.write().await;
        if map.remove(name).is_none() {
            return Ok(false);
        }

        let path = self.skill_file_path(name);
        if path.exists() {
            std::fs::remove_file(&path)
                .with_context(|| format!("failed to delete skill file: {}", path.display()))?;
        }
        info!(skill = %name, "skill deleted");
        Ok(true)
    }

    /// Hot-reload: re-scan directory and refresh in-memory state.
    /// Returns the number of skills loaded.
    pub async fn reload(&self) -> Result<usize> {
        let fresh = load_all(&self.dir)?;
        let count = fresh.len();
        let mut map = self.skills.write().await;
        *map = fresh;
        info!(count, "skill store reloaded");
        Ok(count)
    }

    fn skill_file_path(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{}.yml", name))
    }

    fn write_yaml(&self, config: &SkillYamlConfig) -> Result<()> {
        let path = self.skill_file_path(&config.name);
        let yaml = serde_yaml::to_string(config)
            .with_context(|| format!("failed to serialize skill: {}", config.name))?;
        std::fs::write(&path, yaml)
            .with_context(|| format!("failed to write skill file: {}", path.display()))?;
        Ok(())
    }
}

// ── Directory loading ─────────────────────────────────────────────

/// Scan a directory and load all .yaml/.yml skills into a HashMap.
fn load_all(dir: &Path) -> Result<HashMap<String, SkillYamlConfig>> {
    let mut skills = HashMap::new();

    if !dir.exists() {
        return Ok(skills);
    }

    let entries =
        std::fs::read_dir(dir).with_context(|| format!("failed to read skills dir: {}", dir.display()))?;

    for entry in entries.flatten() {
        let path = entry.path();
        let ext = path.extension().and_then(|e| e.to_str());

        if !matches!(ext, Some("yaml" | "yml")) {
            continue;
        }

        match load_yaml_file(&path) {
            Ok(config) => {
                skills.insert(config.name.clone(), config);
            }
            Err(e) => {
                tracing::warn!(path = %path.display(), error = %e, "skipping skill file");
            }
        }
    }

    Ok(skills)
}

/// Load a single skill from a .yaml/.yml file.
fn load_yaml_file(path: &Path) -> Result<SkillYamlConfig> {
    let content =
        std::fs::read_to_string(path).with_context(|| format!("failed to read: {}", path.display()))?;
    let config: SkillYamlConfig = serde_yaml::from_str(&content)
        .with_context(|| format!("YAML parse error in {}", path.display()))?;
    Ok(config)
}

// ── Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_skill(name: &str) -> SkillYamlConfig {
        SkillYamlConfig {
            name: name.to_string(),
            description: format!("{} description", name),
            prompt: format!("You are a {} skill.", name),
            tags: vec!["test".to_string()],
            version: "1.0.0".to_string(),
        }
    }

    fn setup() -> (TempDir, SkillStore) {
        let tmp = TempDir::new().unwrap();
        let store = SkillStore::new(tmp.path()).unwrap();
        (tmp, store)
    }

    #[tokio::test]
    async fn test_create_and_get() {
        let (_tmp, store) = setup();
        assert!(store.list().await.is_empty());

        let skill = make_skill("summarize");
        let created = store.create(skill).await.unwrap();
        assert_eq!(created.name, "summarize");

        let fetched = store.get("summarize").await.unwrap();
        assert_eq!(fetched.name, "summarize");
        assert_eq!(fetched.description, "summarize description");
    }

    #[tokio::test]
    async fn test_create_duplicate_fails() {
        let (_tmp, store) = setup();
        store.create(make_skill("dup")).await.unwrap();
        let err = store.create(make_skill("dup")).await.unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[tokio::test]
    async fn test_list() {
        let (_tmp, store) = setup();
        store.create(make_skill("alpha")).await.unwrap();
        store.create(make_skill("beta")).await.unwrap();

        let list = store.list().await;
        assert_eq!(list.len(), 2);
        let names: Vec<_> = list.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
    }

    #[tokio::test]
    async fn test_update() {
        let (_tmp, store) = setup();
        store.create(make_skill("updatable")).await.unwrap();

        let mut updated = make_skill("updatable");
        updated.description = "new description".to_string();
        updated.prompt = "Updated prompt.".to_string();

        let result = store.update("updatable", updated).await.unwrap().unwrap();
        assert_eq!(result.description, "new description");

        let fetched = store.get("updatable").await.unwrap();
        assert_eq!(fetched.prompt, "Updated prompt.");
    }

    #[tokio::test]
    async fn test_update_not_found() {
        let (_tmp, store) = setup();
        let result = store.update("ghost", make_skill("ghost")).await.unwrap();
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn test_delete() {
        let (tmp, store) = setup();
        store.create(make_skill("doomed")).await.unwrap();
        assert!(tmp.path().join("doomed.yml").exists());

        assert!(store.delete("doomed").await.unwrap());
        assert!(store.get("doomed").await.is_none());
        assert!(!tmp.path().join("doomed.yml").exists());
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

        let yaml = "name: external-skill\nprompt: \"I was added externally.\"\n";
        std::fs::write(tmp.path().join("external-skill.yml"), yaml).unwrap();

        let count = store.reload().await.unwrap();
        assert_eq!(count, 1);

        let skill = store.get("external-skill").await.unwrap();
        assert_eq!(skill.prompt, "I was added externally.");
    }

    #[tokio::test]
    async fn test_reload_removes_deleted_files() {
        let (tmp, store) = setup();
        store.create(make_skill("ephemeral")).await.unwrap();
        assert_eq!(store.list().await.len(), 1);

        std::fs::remove_file(tmp.path().join("ephemeral.yml")).unwrap();

        let count = store.reload().await.unwrap();
        assert_eq!(count, 0);
        assert!(store.get("ephemeral").await.is_none());
    }

    #[tokio::test]
    async fn test_write_back_produces_valid_yaml() {
        let (tmp, store) = setup();
        store.create(make_skill("roundtrip")).await.unwrap();

        let path = tmp.path().join("roundtrip.yml");
        let content = std::fs::read_to_string(&path).unwrap();
        let parsed: SkillYamlConfig = serde_yaml::from_str(&content).unwrap();
        assert_eq!(parsed.name, "roundtrip");
        assert_eq!(parsed.prompt, "You are a roundtrip skill.");
    }

    #[tokio::test]
    async fn test_update_with_rename() {
        let (tmp, store) = setup();
        store.create(make_skill("old-name")).await.unwrap();
        assert!(tmp.path().join("old-name.yml").exists());

        let mut renamed = make_skill("new-name");
        renamed.description = "renamed skill".to_string();

        let result = store.update("old-name", renamed).await.unwrap().unwrap();
        assert_eq!(result.name, "new-name");

        assert!(!tmp.path().join("old-name.yml").exists());
        assert!(tmp.path().join("new-name.yml").exists());

        assert!(store.get("old-name").await.is_none());
        assert!(store.get("new-name").await.is_some());
    }
}
