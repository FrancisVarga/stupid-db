use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::info;
use uuid::Uuid;

/// Role of a message in a session.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionMessageRole {
    User,
    Agent,
    Team,
    Error,
}

/// A single message in a session conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: String,
    pub role: SessionMessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub execution_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub team_outputs: Option<HashMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agents_used: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub strategy: Option<String>,
}

/// A full session with all messages.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub messages: Vec<SessionMessage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_mode: Option<String>,
}

/// Lightweight session summary (no messages).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_agent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_mode: Option<String>,
}

impl From<&Session> for SessionSummary {
    fn from(session: &Session) -> Self {
        Self {
            id: session.id.clone(),
            name: session.name.clone(),
            created_at: session.created_at,
            updated_at: session.updated_at,
            message_count: session.messages.len(),
            last_agent: session.last_agent.clone(),
            last_mode: session.last_mode.clone(),
        }
    }
}

/// File-based session store — one JSON file per session.
pub struct SessionStore {
    dir: PathBuf,
}

impl SessionStore {
    /// Create a new session store, ensuring the storage directory exists.
    pub fn new(data_dir: &Path) -> Result<Self> {
        let dir = data_dir.join("agent-sessions");
        std::fs::create_dir_all(&dir)
            .with_context(|| format!("failed to create session dir: {}", dir.display()))?;
        info!(path = %dir.display(), "session store initialized");
        Ok(Self { dir })
    }

    fn session_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{}.json", id))
    }

    /// List all sessions sorted by updated_at descending.
    pub fn list(&self) -> Result<Vec<SessionSummary>> {
        let mut summaries = Vec::new();
        for entry in std::fs::read_dir(&self.dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().map_or(false, |ext| ext == "json") {
                match std::fs::read_to_string(&path) {
                    Ok(data) => match serde_json::from_str::<Session>(&data) {
                        Ok(session) => summaries.push(SessionSummary::from(&session)),
                        Err(e) => {
                            tracing::warn!(path = %path.display(), error = %e, "skipping corrupt session");
                        }
                    },
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "failed to read session");
                    }
                }
            }
        }
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }

    /// Get a full session by ID.
    pub fn get(&self, id: &str) -> Result<Option<Session>> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(None);
        }
        let data = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read session: {}", id))?;
        let session = serde_json::from_str(&data)
            .with_context(|| format!("failed to parse session: {}", id))?;
        Ok(Some(session))
    }

    /// Create a new empty session.
    pub fn create(&self, name: Option<&str>) -> Result<Session> {
        let now = Utc::now();
        let default_name = format!("Session {}", now.format("%Y-%m-%d %H:%M"));
        let session = Session {
            id: Uuid::new_v4().to_string(),
            name: name.unwrap_or(&default_name).to_string(),
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
            last_agent: None,
            last_mode: None,
        };
        self.save(&session)?;
        info!(id = %session.id, name = %session.name, "session created");
        Ok(session)
    }

    /// Rename a session.
    pub fn rename(&self, id: &str, name: &str) -> Result<Option<Session>> {
        let Some(mut session) = self.get(id)? else {
            return Ok(None);
        };
        session.name = name.to_string();
        session.updated_at = Utc::now();
        self.save(&session)?;
        Ok(Some(session))
    }

    /// Append a message to a session.
    pub fn append_message(&self, id: &str, msg: SessionMessage) -> Result<Option<Session>> {
        let Some(mut session) = self.get(id)? else {
            return Ok(None);
        };

        // Auto-name from first user message if still default
        if session.messages.is_empty()
            && msg.role == SessionMessageRole::User
            && session.name.starts_with("Session 20")
        {
            let truncated: String = msg.content.chars().take(60).collect();
            session.name = truncated;
        }

        // Track last agent/mode
        match msg.role {
            SessionMessageRole::Agent => {
                session.last_agent = msg.agent_name.clone();
                session.last_mode = Some("agent".to_string());
            }
            SessionMessageRole::Team => {
                session.last_agent = msg.agent_name.clone();
                session.last_mode = Some("team".to_string());
            }
            _ => {}
        }

        session.messages.push(msg);
        session.updated_at = Utc::now();
        self.save(&session)?;
        Ok(Some(session))
    }

    /// Delete a session.
    pub fn delete(&self, id: &str) -> Result<bool> {
        let path = self.session_path(id);
        if !path.exists() {
            return Ok(false);
        }
        std::fs::remove_file(&path)
            .with_context(|| format!("failed to delete session: {}", id))?;
        info!(id = %id, "session deleted");
        Ok(true)
    }

    fn save(&self, session: &Session) -> Result<()> {
        let path = self.session_path(&session.id);
        let data = serde_json::to_string_pretty(session)?;
        std::fs::write(&path, data)
            .with_context(|| format!("failed to write session: {}", session.id))?;
        Ok(())
    }
}
