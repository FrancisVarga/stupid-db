//! SQS queue connection storage with AES-256-GCM encryption at rest.
//!
//! Stores queue connection configs in `{DATA_DIR}/queue-connections.json` with
//! credentials encrypted using AES-256-GCM. Reuses encryption primitives from
//! [`crate::connections`].

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::connections::{decrypt_password, encrypt_password, load_or_generate_key, slugify};

// ── Default value functions ──────────────────────────────────────────

fn default_provider() -> String {
    "sqs".to_string()
}

fn default_enabled() -> bool {
    true
}

fn default_region() -> String {
    "ap-southeast-1".to_string()
}

fn default_poll_interval_ms() -> u64 {
    1000
}

fn default_max_batch_size() -> u32 {
    10
}

fn default_visibility_timeout_secs() -> u32 {
    30
}

fn default_micro_batch_size() -> usize {
    100
}

fn default_micro_batch_timeout_ms() -> u64 {
    1000
}

fn default_color() -> String {
    "#ff8a00".to_string()
}

// ── Public types ─────────────────────────────────────────────────────

/// Full queue connection config with decrypted credentials (internal use only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueConnectionConfig {
    pub id: String,
    pub name: String,
    pub queue_url: String,
    pub dlq_url: Option<String>,
    pub provider: String,
    pub enabled: bool,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub endpoint_url: Option<String>,
    pub poll_interval_ms: u64,
    pub max_batch_size: u32,
    pub visibility_timeout_secs: u32,
    pub micro_batch_size: usize,
    pub micro_batch_timeout_ms: u64,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

/// JSON-safe version with masked credentials (returned by list/get endpoints).
#[derive(Debug, Clone, Serialize)]
pub struct QueueConnectionSafe {
    pub id: String,
    pub name: String,
    pub queue_url: String,
    pub dlq_url: Option<String>,
    pub provider: String,
    pub enabled: bool,
    pub region: String,
    pub access_key_id: &'static str,
    pub secret_access_key: &'static str,
    pub session_token: &'static str,
    pub endpoint_url: Option<String>,
    pub poll_interval_ms: u64,
    pub max_batch_size: u32,
    pub visibility_timeout_secs: u32,
    pub micro_batch_size: usize,
    pub micro_batch_timeout_ms: u64,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&QueueConnectionConfig> for QueueConnectionSafe {
    fn from(c: &QueueConnectionConfig) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            queue_url: c.queue_url.clone(),
            dlq_url: c.dlq_url.clone(),
            provider: c.provider.clone(),
            enabled: c.enabled,
            region: c.region.clone(),
            access_key_id: "********",
            secret_access_key: "********",
            session_token: "********",
            endpoint_url: c.endpoint_url.clone(),
            poll_interval_ms: c.poll_interval_ms,
            max_batch_size: c.max_batch_size,
            visibility_timeout_secs: c.visibility_timeout_secs,
            micro_batch_size: c.micro_batch_size,
            micro_batch_timeout_ms: c.micro_batch_timeout_ms,
            color: c.color.clone(),
            created_at: c.created_at.clone(),
            updated_at: c.updated_at.clone(),
        }
    }
}

/// Decrypted credentials for SqsConsumer creation.
#[derive(Debug, Clone, Serialize)]
pub struct QueueConnectionCredentials {
    pub id: String,
    pub name: String,
    pub queue_url: String,
    pub dlq_url: Option<String>,
    pub provider: String,
    pub region: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub endpoint_url: Option<String>,
}

impl From<&QueueConnectionConfig> for QueueConnectionCredentials {
    fn from(c: &QueueConnectionConfig) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            queue_url: c.queue_url.clone(),
            dlq_url: c.dlq_url.clone(),
            provider: c.provider.clone(),
            region: c.region.clone(),
            access_key_id: c.access_key_id.clone(),
            secret_access_key: c.secret_access_key.clone(),
            session_token: c.session_token.clone(),
            endpoint_url: c.endpoint_url.clone(),
        }
    }
}

impl QueueConnectionConfig {
    /// Build an `AwsConfig` suitable for `SqsConsumer::new()`.
    pub fn to_aws_config(&self) -> stupid_core::config::AwsConfig {
        stupid_core::config::AwsConfig {
            region: self.region.clone(),
            access_key_id: if self.access_key_id.is_empty() { None } else { Some(self.access_key_id.clone()) },
            secret_access_key: if self.secret_access_key.is_empty() { None } else { Some(self.secret_access_key.clone()) },
            session_token: if self.session_token.is_empty() { None } else { Some(self.session_token.clone()) },
            s3_bucket: None,
            s3_prefix: None,
            endpoint_url: self.endpoint_url.clone(),
        }
    }

    /// Build a `QueueConfig` suitable for `SqsConsumer::new()`.
    pub fn to_queue_config(&self) -> stupid_core::config::QueueConfig {
        stupid_core::config::QueueConfig {
            enabled: self.enabled,
            provider: self.provider.clone(),
            queue_url: self.queue_url.clone(),
            poll_interval_ms: self.poll_interval_ms,
            max_batch_size: self.max_batch_size,
            visibility_timeout_secs: self.visibility_timeout_secs,
            micro_batch_size: self.micro_batch_size,
            micro_batch_timeout_ms: self.micro_batch_timeout_ms,
            dlq_url: self.dlq_url.clone(),
            aws: self.to_aws_config(),
        }
    }
}

/// User input for creating/updating a queue connection.
#[derive(Debug, Clone, Deserialize)]
pub struct QueueConnectionInput {
    pub name: String,
    pub queue_url: String,
    #[serde(default)]
    pub dlq_url: Option<String>,
    #[serde(default = "default_provider")]
    pub provider: String,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: String,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    #[serde(default = "default_poll_interval_ms")]
    pub poll_interval_ms: u64,
    #[serde(default = "default_max_batch_size")]
    pub max_batch_size: u32,
    #[serde(default = "default_visibility_timeout_secs")]
    pub visibility_timeout_secs: u32,
    #[serde(default = "default_micro_batch_size")]
    pub micro_batch_size: usize,
    #[serde(default = "default_micro_batch_timeout_ms")]
    pub micro_batch_timeout_ms: u64,
    #[serde(default = "default_color")]
    pub color: String,
}

// ── On-disk format ───────────────────────────────────────────────────

/// On-disk format: credentials stored as encrypted hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredQueueConnection {
    id: String,
    name: String,
    queue_url: String,
    dlq_url: Option<String>,
    provider: String,
    enabled: bool,
    region: String,
    encrypted_access_key_id: String,
    encrypted_secret_access_key: String,
    encrypted_session_token: String,
    endpoint_url: Option<String>,
    poll_interval_ms: u64,
    max_batch_size: u32,
    visibility_timeout_secs: u32,
    micro_batch_size: usize,
    micro_batch_timeout_ms: u64,
    color: String,
    created_at: String,
    updated_at: String,
}

// ── Store ────────────────────────────────────────────────────────────

/// Thread-safe queue connection credential store.
pub struct QueueConnectionStore {
    data_dir: PathBuf,
    key: [u8; 32],
}

impl QueueConnectionStore {
    /// Create a new store, loading or generating the encryption key.
    pub fn new(data_dir: &PathBuf) -> anyhow::Result<Self> {
        let key = load_or_generate_key(data_dir)?;
        Ok(Self {
            data_dir: data_dir.clone(),
            key,
        })
    }

    fn queue_connections_path(&self) -> PathBuf {
        self.data_dir.join("queue-connections.json")
    }

    fn load_stored(&self) -> anyhow::Result<Vec<StoredQueueConnection>> {
        let path = self.queue_connections_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let connections: Vec<StoredQueueConnection> = serde_json::from_str(&data)?;
        Ok(connections)
    }

    fn save_stored(&self, connections: &[StoredQueueConnection]) -> anyhow::Result<()> {
        let path = self.queue_connections_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(connections)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    fn decrypt_connection(
        &self,
        stored: &StoredQueueConnection,
    ) -> anyhow::Result<QueueConnectionConfig> {
        let access_key_id = decrypt_password(&self.key, &stored.encrypted_access_key_id)?;
        let secret_access_key = decrypt_password(&self.key, &stored.encrypted_secret_access_key)?;
        let session_token = decrypt_password(&self.key, &stored.encrypted_session_token)?;
        Ok(QueueConnectionConfig {
            id: stored.id.clone(),
            name: stored.name.clone(),
            queue_url: stored.queue_url.clone(),
            dlq_url: stored.dlq_url.clone(),
            provider: stored.provider.clone(),
            enabled: stored.enabled,
            region: stored.region.clone(),
            access_key_id,
            secret_access_key,
            session_token,
            endpoint_url: stored.endpoint_url.clone(),
            poll_interval_ms: stored.poll_interval_ms,
            max_batch_size: stored.max_batch_size,
            visibility_timeout_secs: stored.visibility_timeout_secs,
            micro_batch_size: stored.micro_batch_size,
            micro_batch_timeout_ms: stored.micro_batch_timeout_ms,
            color: stored.color.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        })
    }

    /// List all queue connections with full decrypted configs (for consumer spawning).
    pub fn list_configs(&self) -> anyhow::Result<Vec<QueueConnectionConfig>> {
        let stored = self.load_stored()?;
        let mut result = Vec::with_capacity(stored.len());
        for s in &stored {
            result.push(self.decrypt_connection(s)?);
        }
        Ok(result)
    }

    /// List all queue connections with masked credentials.
    pub fn list(&self) -> anyhow::Result<Vec<QueueConnectionSafe>> {
        let stored = self.load_stored()?;
        let mut result = Vec::with_capacity(stored.len());
        for s in &stored {
            match self.decrypt_connection(s) {
                Ok(c) => result.push(QueueConnectionSafe::from(&c)),
                Err(e) => {
                    warn!("Failed to decrypt queue connection '{}': {}", s.id, e);
                    result.push(QueueConnectionSafe {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        queue_url: s.queue_url.clone(),
                        dlq_url: s.dlq_url.clone(),
                        provider: s.provider.clone(),
                        enabled: s.enabled,
                        region: s.region.clone(),
                        access_key_id: "********",
                        secret_access_key: "********",
                        session_token: "********",
                        endpoint_url: s.endpoint_url.clone(),
                        poll_interval_ms: s.poll_interval_ms,
                        max_batch_size: s.max_batch_size,
                        visibility_timeout_secs: s.visibility_timeout_secs,
                        micro_batch_size: s.micro_batch_size,
                        micro_batch_timeout_ms: s.micro_batch_timeout_ms,
                        color: s.color.clone(),
                        created_at: s.created_at.clone(),
                        updated_at: s.updated_at.clone(),
                    });
                }
            }
        }
        Ok(result)
    }

    /// Get a single queue connection by ID with masked credentials.
    pub fn get_safe(&self, id: &str) -> anyhow::Result<Option<QueueConnectionSafe>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| s.id == id) {
            Some(s) => {
                let c = self.decrypt_connection(s)?;
                Ok(Some(QueueConnectionSafe::from(&c)))
            }
            None => Ok(None),
        }
    }

    /// Get a single queue connection by ID with decrypted credentials (internal use).
    pub fn get(&self, id: &str) -> anyhow::Result<Option<QueueConnectionConfig>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| s.id == id) {
            Some(s) => Ok(Some(self.decrypt_connection(s)?)),
            None => Ok(None),
        }
    }

    /// Get decrypted credentials for SqsConsumer creation.
    pub fn get_credentials(
        &self,
        id: &str,
    ) -> anyhow::Result<Option<QueueConnectionCredentials>> {
        self.get(id)
            .map(|opt| opt.map(|c| QueueConnectionCredentials::from(&c)))
    }

    /// Add a new queue connection. Returns the created connection (safe).
    pub fn add(&self, input: &QueueConnectionInput) -> anyhow::Result<QueueConnectionSafe> {
        let mut stored = self.load_stored()?;
        let id = slugify(&input.name);

        // Check for duplicate ID.
        if stored.iter().any(|s| s.id == id) {
            anyhow::bail!("Queue connection with id '{}' already exists", id);
        }

        let now = chrono::Utc::now().to_rfc3339();
        let encrypted_access_key_id = encrypt_password(&self.key, &input.access_key_id)?;
        let encrypted_secret_access_key = encrypt_password(&self.key, &input.secret_access_key)?;
        let encrypted_session_token = encrypt_password(&self.key, &input.session_token)?;

        stored.push(StoredQueueConnection {
            id: id.clone(),
            name: input.name.clone(),
            queue_url: input.queue_url.clone(),
            dlq_url: input.dlq_url.clone(),
            provider: input.provider.clone(),
            enabled: input.enabled,
            region: input.region.clone(),
            encrypted_access_key_id,
            encrypted_secret_access_key,
            encrypted_session_token,
            endpoint_url: input.endpoint_url.clone(),
            poll_interval_ms: input.poll_interval_ms,
            max_batch_size: input.max_batch_size,
            visibility_timeout_secs: input.visibility_timeout_secs,
            micro_batch_size: input.micro_batch_size,
            micro_batch_timeout_ms: input.micro_batch_timeout_ms,
            color: input.color.clone(),
            created_at: now.clone(),
            updated_at: now.clone(),
        });
        self.save_stored(&stored)?;

        info!(
            "Added queue connection '{}' (queue_url: {})",
            id, input.queue_url
        );
        let config = QueueConnectionConfig {
            id,
            name: input.name.clone(),
            queue_url: input.queue_url.clone(),
            dlq_url: input.dlq_url.clone(),
            provider: input.provider.clone(),
            enabled: input.enabled,
            region: input.region.clone(),
            access_key_id: input.access_key_id.clone(),
            secret_access_key: input.secret_access_key.clone(),
            session_token: input.session_token.clone(),
            endpoint_url: input.endpoint_url.clone(),
            poll_interval_ms: input.poll_interval_ms,
            max_batch_size: input.max_batch_size,
            visibility_timeout_secs: input.visibility_timeout_secs,
            micro_batch_size: input.micro_batch_size,
            micro_batch_timeout_ms: input.micro_batch_timeout_ms,
            color: input.color.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        Ok(QueueConnectionSafe::from(&config))
    }

    /// Update an existing queue connection. Returns the updated connection (safe).
    ///
    /// If a credential field (access_key_id, secret_access_key, session_token) is
    /// empty, the existing encrypted value is preserved.
    pub fn update(
        &self,
        id: &str,
        input: &QueueConnectionInput,
    ) -> anyhow::Result<Option<QueueConnectionSafe>> {
        let mut stored = self.load_stored()?;
        let idx = match stored.iter().position(|s| s.id == id) {
            Some(i) => i,
            None => return Ok(None),
        };

        let created_at = stored[idx].created_at.clone();
        let now = chrono::Utc::now().to_rfc3339();

        // Preserve existing encrypted credentials when input is empty.
        let encrypted_access_key_id = if input.access_key_id.is_empty() {
            stored[idx].encrypted_access_key_id.clone()
        } else {
            encrypt_password(&self.key, &input.access_key_id)?
        };
        let encrypted_secret_access_key = if input.secret_access_key.is_empty() {
            stored[idx].encrypted_secret_access_key.clone()
        } else {
            encrypt_password(&self.key, &input.secret_access_key)?
        };
        let encrypted_session_token = if input.session_token.is_empty() {
            stored[idx].encrypted_session_token.clone()
        } else {
            encrypt_password(&self.key, &input.session_token)?
        };

        // Resolve the actual credential values for the return type.
        let actual_access_key_id = if input.access_key_id.is_empty() {
            decrypt_password(&self.key, &stored[idx].encrypted_access_key_id)?
        } else {
            input.access_key_id.clone()
        };
        let actual_secret_access_key = if input.secret_access_key.is_empty() {
            decrypt_password(&self.key, &stored[idx].encrypted_secret_access_key)?
        } else {
            input.secret_access_key.clone()
        };
        let actual_session_token = if input.session_token.is_empty() {
            decrypt_password(&self.key, &stored[idx].encrypted_session_token)?
        } else {
            input.session_token.clone()
        };

        stored[idx] = StoredQueueConnection {
            id: id.to_string(),
            name: input.name.clone(),
            queue_url: input.queue_url.clone(),
            dlq_url: input.dlq_url.clone(),
            provider: input.provider.clone(),
            enabled: input.enabled,
            region: input.region.clone(),
            encrypted_access_key_id,
            encrypted_secret_access_key,
            encrypted_session_token,
            endpoint_url: input.endpoint_url.clone(),
            poll_interval_ms: input.poll_interval_ms,
            max_batch_size: input.max_batch_size,
            visibility_timeout_secs: input.visibility_timeout_secs,
            micro_batch_size: input.micro_batch_size,
            micro_batch_timeout_ms: input.micro_batch_timeout_ms,
            color: input.color.clone(),
            created_at: created_at.clone(),
            updated_at: now.clone(),
        };
        self.save_stored(&stored)?;

        info!("Updated queue connection '{}'", id);
        let config = QueueConnectionConfig {
            id: id.to_string(),
            name: input.name.clone(),
            queue_url: input.queue_url.clone(),
            dlq_url: input.dlq_url.clone(),
            provider: input.provider.clone(),
            enabled: input.enabled,
            region: input.region.clone(),
            access_key_id: actual_access_key_id,
            secret_access_key: actual_secret_access_key,
            session_token: actual_session_token,
            endpoint_url: input.endpoint_url.clone(),
            poll_interval_ms: input.poll_interval_ms,
            max_batch_size: input.max_batch_size,
            visibility_timeout_secs: input.visibility_timeout_secs,
            micro_batch_size: input.micro_batch_size,
            micro_batch_timeout_ms: input.micro_batch_timeout_ms,
            color: input.color.clone(),
            created_at,
            updated_at: now,
        };
        Ok(Some(QueueConnectionSafe::from(&config)))
    }

    /// Delete a queue connection by ID. Returns true if it existed.
    pub fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut stored = self.load_stored()?;
        let len_before = stored.len();
        stored.retain(|s| s.id != id);
        if stored.len() == len_before {
            return Ok(false);
        }
        self.save_stored(&stored)?;
        info!("Deleted queue connection '{}'", id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(name: &str) -> QueueConnectionInput {
        QueueConnectionInput {
            name: name.to_string(),
            queue_url: "https://sqs.ap-southeast-1.amazonaws.com/123456789/test-queue".to_string(),
            dlq_url: Some(
                "https://sqs.ap-southeast-1.amazonaws.com/123456789/test-queue-dlq".to_string(),
            ),
            provider: default_provider(),
            enabled: default_enabled(),
            region: default_region(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: "FwoGZXIvYXdzEBYaDH7example".to_string(),
            endpoint_url: None,
            poll_interval_ms: default_poll_interval_ms(),
            max_batch_size: default_max_batch_size(),
            visibility_timeout_secs: default_visibility_timeout_secs(),
            micro_batch_size: default_micro_batch_size(),
            micro_batch_timeout_ms: default_micro_batch_timeout_ms(),
            color: default_color(),
        }
    }

    #[test]
    fn test_add_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Production Queue");
        let safe = store.add(&input).unwrap();

        assert_eq!(safe.id, "production-queue");
        assert_eq!(safe.access_key_id, "********");
        assert_eq!(safe.secret_access_key, "********");
        assert_eq!(safe.session_token, "********");
        assert_eq!(safe.queue_url, input.queue_url);

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Production Queue");
        assert_eq!(list[0].access_key_id, "********");
    }

    #[test]
    fn test_get_credentials() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Creds Queue");
        store.add(&input).unwrap();

        let creds = store.get_credentials("creds-queue").unwrap().unwrap();
        assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(
            creds.secret_access_key,
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        );
        assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
        assert_eq!(creds.queue_url, input.queue_url);
    }

    #[test]
    fn test_update() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Update Queue");
        store.add(&input).unwrap();

        let mut updated_input = make_input("Update Queue");
        updated_input.queue_url =
            "https://sqs.us-east-1.amazonaws.com/123456789/new-queue".to_string();
        updated_input.access_key_id = "NEWKEY123".to_string();

        let updated = store.update("update-queue", &updated_input).unwrap().unwrap();
        assert_eq!(updated.queue_url, updated_input.queue_url);

        let full = store.get("update-queue").unwrap().unwrap();
        assert_eq!(full.queue_url, updated_input.queue_url);
        assert_eq!(full.access_key_id, "NEWKEY123");
    }

    #[test]
    fn test_update_preserves_credentials() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Preserve Queue");
        store.add(&input).unwrap();

        // Update with empty credential fields -- should preserve originals.
        let mut updated_input = make_input("Preserve Queue");
        updated_input.queue_url =
            "https://sqs.us-east-1.amazonaws.com/123456789/changed-queue".to_string();
        updated_input.access_key_id = String::new();
        updated_input.secret_access_key = String::new();
        updated_input.session_token = String::new();

        store
            .update("preserve-queue", &updated_input)
            .unwrap()
            .unwrap();

        let creds = store
            .get_credentials("preserve-queue")
            .unwrap()
            .unwrap();
        assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(
            creds.secret_access_key,
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        );
        assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
        assert_eq!(creds.queue_url, updated_input.queue_url);
    }

    #[test]
    fn test_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Delete Queue");
        store.add(&input).unwrap();

        assert!(store.delete("delete-queue").unwrap());
        assert!(!store.delete("delete-queue").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_duplicate_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = QueueConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Dupe Queue");
        store.add(&input).unwrap();

        let result = store.add(&input);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("already exists")
        );
    }
}
