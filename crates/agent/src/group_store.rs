use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

/// A named group of agents with metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentGroup {
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub color: String,
    #[serde(default)]
    pub agent_names: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// File-based store for agent groups — single JSON file for all groups.
pub struct AgentGroupStore {
    path: PathBuf,
}

impl AgentGroupStore {
    /// Create a new group store, ensuring the storage directory exists.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("agent-groups");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create agent-groups dir: {}", dir.display()))?;
        let path = dir.join("groups.json");
        info!(path = %path.display(), "agent group store initialized");
        Ok(Self { path })
    }

    /// List all groups.
    pub fn list(&self) -> Result<Vec<AgentGroup>> {
        self.load()
    }

    /// Get a group by name.
    pub fn get(&self, name: &str) -> Result<Option<AgentGroup>> {
        let groups = self.load()?;
        Ok(groups.into_iter().find(|g| g.name == name))
    }

    /// Create a new group. Returns error if a group with that name already exists.
    pub fn create(&self, name: &str, description: &str, color: &str) -> Result<AgentGroup> {
        let mut groups = self.load()?;
        if groups.iter().any(|g| g.name == name) {
            anyhow::bail!("group already exists: {}", name);
        }
        let now = Utc::now();
        let group = AgentGroup {
            name: name.to_string(),
            description: description.to_string(),
            color: color.to_string(),
            agent_names: Vec::new(),
            created_at: now,
            updated_at: now,
        };
        groups.push(group.clone());
        self.save(&groups)?;
        info!(name = %name, "agent group created");
        Ok(group)
    }

    /// Update a group's description and/or color. Returns None if group not found.
    pub fn update(
        &self,
        name: &str,
        description: Option<&str>,
        color: Option<&str>,
    ) -> Result<Option<AgentGroup>> {
        let mut groups = self.load()?;
        let Some(group) = groups.iter_mut().find(|g| g.name == name) else {
            return Ok(None);
        };
        if let Some(desc) = description {
            group.description = desc.to_string();
        }
        if let Some(c) = color {
            group.color = c.to_string();
        }
        group.updated_at = Utc::now();
        let updated = group.clone();
        self.save(&groups)?;
        info!(name = %name, "agent group updated");
        Ok(Some(updated))
    }

    /// Delete a group by name. Returns true if it existed.
    pub fn delete(&self, name: &str) -> Result<bool> {
        let mut groups = self.load()?;
        let len_before = groups.len();
        groups.retain(|g| g.name != name);
        if groups.len() == len_before {
            return Ok(false);
        }
        self.save(&groups)?;
        info!(name = %name, "agent group deleted");
        Ok(true)
    }

    /// Add an agent to a group. Returns None if group not found.
    pub fn add_agent(&self, group_name: &str, agent_name: &str) -> Result<Option<AgentGroup>> {
        let mut groups = self.load()?;
        let Some(group) = groups.iter_mut().find(|g| g.name == group_name) else {
            return Ok(None);
        };
        if !group.agent_names.contains(&agent_name.to_string()) {
            group.agent_names.push(agent_name.to_string());
            group.updated_at = Utc::now();
        }
        let updated = group.clone();
        self.save(&groups)?;
        info!(group = %group_name, agent = %agent_name, "agent added to group");
        Ok(Some(updated))
    }

    /// Remove an agent from a group. Returns None if group not found.
    pub fn remove_agent(&self, group_name: &str, agent_name: &str) -> Result<Option<AgentGroup>> {
        let mut groups = self.load()?;
        let Some(group) = groups.iter_mut().find(|g| g.name == group_name) else {
            return Ok(None);
        };
        group.agent_names.retain(|n| n != agent_name);
        group.updated_at = Utc::now();
        let updated = group.clone();
        self.save(&groups)?;
        info!(group = %group_name, agent = %agent_name, "agent removed from group");
        Ok(Some(updated))
    }

    /// Find all group names that contain a given agent.
    pub fn groups_for_agent(&self, agent_name: &str) -> Result<Vec<String>> {
        let groups = self.load()?;
        Ok(groups
            .into_iter()
            .filter(|g| g.agent_names.contains(&agent_name.to_string()))
            .map(|g| g.name)
            .collect())
    }

    fn load(&self) -> Result<Vec<AgentGroup>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&self.path)
            .with_context(|| format!("failed to read groups file: {}", self.path.display()))?;
        let groups: Vec<AgentGroup> = serde_json::from_str(&data)
            .with_context(|| "failed to parse groups.json")?;
        Ok(groups)
    }

    fn save(&self, groups: &[AgentGroup]) -> Result<()> {
        let data = serde_json::to_string_pretty(groups)?;
        std::fs::write(&self.path, data)
            .with_context(|| format!("failed to write groups file: {}", self.path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, AgentGroupStore) {
        let tmp = TempDir::new().unwrap();
        let store = AgentGroupStore::new(tmp.path()).unwrap();
        (tmp, store)
    }

    #[test]
    fn test_create_and_list() {
        let (_tmp, store) = setup();
        assert!(store.list().unwrap().is_empty());

        let group = store.create("monitors", "Monitoring agents", "#00f0ff").unwrap();
        assert_eq!(group.name, "monitors");
        assert_eq!(group.description, "Monitoring agents");
        assert_eq!(group.color, "#00f0ff");
        assert!(group.agent_names.is_empty());

        let all = store.list().unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "monitors");
    }

    #[test]
    fn test_create_duplicate_fails() {
        let (_tmp, store) = setup();
        store.create("monitors", "desc", "#fff").unwrap();
        let err = store.create("monitors", "other", "#000").unwrap_err();
        assert!(err.to_string().contains("already exists"));
    }

    #[test]
    fn test_get() {
        let (_tmp, store) = setup();
        assert!(store.get("nope").unwrap().is_none());

        store.create("monitors", "desc", "#fff").unwrap();
        let group = store.get("monitors").unwrap().unwrap();
        assert_eq!(group.name, "monitors");
    }

    #[test]
    fn test_update() {
        let (_tmp, store) = setup();
        store.create("monitors", "desc", "#fff").unwrap();

        let updated = store.update("monitors", Some("new desc"), None).unwrap().unwrap();
        assert_eq!(updated.description, "new desc");
        assert_eq!(updated.color, "#fff");

        let updated = store.update("monitors", None, Some("#000")).unwrap().unwrap();
        assert_eq!(updated.description, "new desc");
        assert_eq!(updated.color, "#000");

        assert!(store.update("nope", Some("x"), None).unwrap().is_none());
    }

    #[test]
    fn test_delete() {
        let (_tmp, store) = setup();
        assert!(!store.delete("nope").unwrap());

        store.create("monitors", "desc", "#fff").unwrap();
        assert!(store.delete("monitors").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_add_and_remove_agent() {
        let (_tmp, store) = setup();
        store.create("monitors", "desc", "#fff").unwrap();

        // Add agent
        let group = store.add_agent("monitors", "agent-1").unwrap().unwrap();
        assert_eq!(group.agent_names, vec!["agent-1"]);

        // Idempotent add
        let group = store.add_agent("monitors", "agent-1").unwrap().unwrap();
        assert_eq!(group.agent_names, vec!["agent-1"]);

        // Add second agent
        let group = store.add_agent("monitors", "agent-2").unwrap().unwrap();
        assert_eq!(group.agent_names, vec!["agent-1", "agent-2"]);

        // Remove agent
        let group = store.remove_agent("monitors", "agent-1").unwrap().unwrap();
        assert_eq!(group.agent_names, vec!["agent-2"]);

        // Group not found
        assert!(store.add_agent("nope", "agent-1").unwrap().is_none());
        assert!(store.remove_agent("nope", "agent-1").unwrap().is_none());
    }

    #[test]
    fn test_groups_for_agent() {
        let (_tmp, store) = setup();
        store.create("monitors", "desc", "#fff").unwrap();
        store.create("alerts", "desc", "#f00").unwrap();

        store.add_agent("monitors", "agent-1").unwrap();
        store.add_agent("alerts", "agent-1").unwrap();
        store.add_agent("monitors", "agent-2").unwrap();

        let mut groups = store.groups_for_agent("agent-1").unwrap();
        groups.sort();
        assert_eq!(groups, vec!["alerts", "monitors"]);

        let groups = store.groups_for_agent("agent-2").unwrap();
        assert_eq!(groups, vec!["monitors"]);

        let groups = store.groups_for_agent("agent-3").unwrap();
        assert!(groups.is_empty());
    }
}
