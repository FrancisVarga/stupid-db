use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use stupid_tool_runtime::conversation::ConversationMessage;
use tracing::debug;

use crate::config::CliConfig;

/// Persisted session metadata and conversation history.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    /// Unique session identifier (sanitized from first message or UUID)
    pub id: String,
    /// Human-readable session name
    pub name: String,
    /// LLM provider used
    pub provider: String,
    /// Model used
    pub model: String,
    /// System prompt (if any)
    pub system_prompt: Option<String>,
    /// When the session was created
    pub created_at: DateTime<Utc>,
    /// When the session was last updated
    pub updated_at: DateTime<Utc>,
    /// Conversation messages
    pub messages: Vec<ConversationMessage>,
}

impl Session {
    /// Create a new session with auto-generated ID.
    pub fn new(provider: String, model: String, system_prompt: Option<String>) -> Self {
        let now = Utc::now();
        let id = now.format("%Y%m%d-%H%M%S").to_string();
        Self {
            id: id.clone(),
            name: id,
            provider,
            model,
            system_prompt,
            created_at: now,
            updated_at: now,
            messages: Vec::new(),
        }
    }

    /// Update the session name based on the first user message.
    pub fn update_name_from_first_message(&mut self) {
        if let Some(ConversationMessage::User(text)) = self.messages.first() {
            let sanitized = sanitize_session_name(text);
            if !sanitized.is_empty() {
                self.name = sanitized;
            }
        }
    }

    /// Return the file path for this session.
    pub fn file_path(&self) -> Result<PathBuf> {
        let dir = CliConfig::ensure_sessions_dir()?;
        Ok(dir.join(format!("{}.json", self.id)))
    }

    /// Save the session to disk.
    pub fn save(&mut self) -> Result<()> {
        self.updated_at = Utc::now();
        let path = self.file_path()?;
        let json = serde_json::to_string_pretty(self)
            .context("failed to serialize session")?;
        std::fs::write(&path, json)
            .with_context(|| format!("failed to write session: {}", path.display()))?;
        debug!(id = %self.id, path = %path.display(), "Session saved");
        Ok(())
    }

    /// Load a session from disk by ID or name prefix.
    pub fn load(id_or_name: &str) -> Result<Self> {
        let sessions_dir = CliConfig::ensure_sessions_dir()?;

        // Try exact ID match first
        let exact_path = sessions_dir.join(format!("{}.json", id_or_name));
        if exact_path.exists() {
            return Self::load_from_path(&exact_path);
        }

        // Try prefix match on ID or name
        let entries = std::fs::read_dir(&sessions_dir)
            .context("failed to read sessions directory")?;

        let mut matches = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(session) = Self::load_from_path(&path) {
                    if session.id.starts_with(id_or_name)
                        || session.name.to_lowercase().contains(&id_or_name.to_lowercase())
                    {
                        matches.push(session);
                    }
                }
            }
        }

        match matches.len() {
            0 => anyhow::bail!("no session found matching '{}'", id_or_name),
            1 => Ok(matches.into_iter().next().unwrap()),
            n => anyhow::bail!(
                "ambiguous session '{}': {} matches found. Use a more specific identifier.",
                id_or_name,
                n
            ),
        }
    }

    /// Load a session from a specific file path.
    fn load_from_path(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read session: {}", path.display()))?;
        let session: Self = serde_json::from_str(&content)
            .with_context(|| format!("failed to parse session: {}", path.display()))?;
        Ok(session)
    }

    /// List all saved sessions, sorted by most recent first.
    pub fn list_all() -> Result<Vec<SessionSummary>> {
        let sessions_dir = CliConfig::ensure_sessions_dir()?;
        let entries = std::fs::read_dir(&sessions_dir)
            .context("failed to read sessions directory")?;

        let mut summaries = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                if let Ok(session) = Self::load_from_path(&path) {
                    summaries.push(SessionSummary {
                        id: session.id,
                        name: session.name,
                        provider: session.provider,
                        model: session.model,
                        created_at: session.created_at,
                        updated_at: session.updated_at,
                        message_count: session.messages.len(),
                    });
                }
            }
        }

        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        Ok(summaries)
    }
}

/// Lightweight summary of a session for listing.
#[derive(Debug)]
#[allow(dead_code)]
pub struct SessionSummary {
    pub id: String,
    pub name: String,
    pub provider: String,
    pub model: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub message_count: usize,
}

/// Sanitize a user message into a valid session name.
/// Takes the first ~50 chars, replaces non-alphanumeric with dashes, lowercases.
fn sanitize_session_name(text: &str) -> String {
    let truncated: String = text.chars().take(50).collect();
    let sanitized: String = truncated
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    // Collapse multiple dashes and trim
    let mut result = String::new();
    let mut prev_dash = false;
    for c in sanitized.chars() {
        if c == '-' {
            if !prev_dash && !result.is_empty() {
                result.push('-');
            }
            prev_dash = true;
        } else {
            result.push(c);
            prev_dash = false;
        }
    }
    result.trim_end_matches('-').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_session_name() {
        assert_eq!(sanitize_session_name("Hello World!"), "hello-world");
        assert_eq!(
            sanitize_session_name("What is the meaning of life?"),
            "what-is-the-meaning-of-life"
        );
        assert_eq!(
            sanitize_session_name("   lots   of   spaces   "),
            "lots-of-spaces"
        );
        assert_eq!(sanitize_session_name("simple"), "simple");
    }

    #[test]
    fn test_sanitize_long_name() {
        let long = "a".repeat(100);
        let result = sanitize_session_name(&long);
        assert!(result.len() <= 50);
    }

    #[test]
    fn test_session_new() {
        let session = Session::new(
            "claude".to_string(),
            "claude-sonnet-4-20250514".to_string(),
            None,
        );
        assert!(!session.id.is_empty());
        assert_eq!(session.provider, "claude");
        assert!(session.messages.is_empty());
    }
}
