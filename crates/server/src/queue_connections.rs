//! SQS queue connection storage with AES-256-GCM encryption at rest.
//!
//! Stores queue connection configs in `{DATA_DIR}/queue-connections.json` with
//! credentials encrypted using AES-256-GCM. Implements [`CredentialStore`] for
//! shared CRUD logic.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::credential_store::{
    decrypt_password, encrypt_password, load_or_generate_key, slugify, CredentialStore,
};

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
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct QueueConnectionSafe {
    pub id: String,
    pub name: String,
    pub queue_url: String,
    pub dlq_url: Option<String>,
    pub provider: String,
    pub enabled: bool,
    pub region: String,
    #[schema(value_type = String)]
    pub access_key_id: &'static str,
    #[schema(value_type = String)]
    pub secret_access_key: &'static str,
    #[schema(value_type = String)]
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

/// Decrypted credentials for SqsConsumer creation.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
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

impl QueueConnectionConfig {
    /// Build an `AwsConfig` suitable for `SqsConsumer::new()`.
    pub fn to_aws_config(&self) -> stupid_core::config::AwsConfig {
        stupid_core::config::AwsConfig {
            region: self.region.clone(),
            access_key_id: if self.access_key_id.is_empty() {
                None
            } else {
                Some(self.access_key_id.clone())
            },
            secret_access_key: if self.secret_access_key.is_empty() {
                None
            } else {
                Some(self.secret_access_key.clone())
            },
            session_token: if self.session_token.is_empty() {
                None
            } else {
                Some(self.session_token.clone())
            },
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
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
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
pub(crate) struct StoredQueueConnection {
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

    /// List all queue connections with full decrypted configs (for consumer spawning).
    pub fn list_configs(&self) -> anyhow::Result<Vec<QueueConnectionConfig>> {
        let stored = self.load_stored()?;
        let mut result = Vec::with_capacity(stored.len());
        for s in &stored {
            result.push(self.decrypt_record(s)?);
        }
        Ok(result)
    }
}

impl CredentialStore for QueueConnectionStore {
    type Config = QueueConnectionConfig;
    type Safe = QueueConnectionSafe;
    type Credentials = QueueConnectionCredentials;
    type Input = QueueConnectionInput;
    type Stored = StoredQueueConnection;

    fn store_path(&self) -> PathBuf {
        self.data_dir.join("queue-connections.json")
    }

    fn type_name() -> &'static str {
        "queue connection"
    }

    fn generate_id(input: &Self::Input) -> String {
        slugify(&input.name)
    }

    fn stored_id(stored: &Self::Stored) -> &str {
        &stored.id
    }

    fn stored_created_at(stored: &Self::Stored) -> &str {
        &stored.created_at
    }

    fn encrypt_record(
        &self,
        id: &str,
        input: &Self::Input,
        created_at: &str,
        updated_at: &str,
    ) -> anyhow::Result<Self::Stored> {
        let encrypted_access_key_id = encrypt_password(&self.key, &input.access_key_id)?;
        let encrypted_secret_access_key =
            encrypt_password(&self.key, &input.secret_access_key)?;
        let encrypted_session_token = encrypt_password(&self.key, &input.session_token)?;

        Ok(StoredQueueConnection {
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
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        })
    }

    fn encrypt_record_update(
        &self,
        id: &str,
        input: &Self::Input,
        existing: &Self::Stored,
        created_at: &str,
        updated_at: &str,
    ) -> anyhow::Result<Self::Stored> {
        // Preserve existing encrypted credentials when input is empty.
        let encrypted_access_key_id = if input.access_key_id.is_empty() {
            existing.encrypted_access_key_id.clone()
        } else {
            encrypt_password(&self.key, &input.access_key_id)?
        };
        let encrypted_secret_access_key = if input.secret_access_key.is_empty() {
            existing.encrypted_secret_access_key.clone()
        } else {
            encrypt_password(&self.key, &input.secret_access_key)?
        };
        let encrypted_session_token = if input.session_token.is_empty() {
            existing.encrypted_session_token.clone()
        } else {
            encrypt_password(&self.key, &input.session_token)?
        };

        Ok(StoredQueueConnection {
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
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        })
    }

    fn decrypt_record(&self, stored: &Self::Stored) -> anyhow::Result<Self::Config> {
        let access_key_id = decrypt_password(&self.key, &stored.encrypted_access_key_id)?;
        let secret_access_key =
            decrypt_password(&self.key, &stored.encrypted_secret_access_key)?;
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

    fn config_to_safe(config: &Self::Config) -> Self::Safe {
        QueueConnectionSafe {
            id: config.id.clone(),
            name: config.name.clone(),
            queue_url: config.queue_url.clone(),
            dlq_url: config.dlq_url.clone(),
            provider: config.provider.clone(),
            enabled: config.enabled,
            region: config.region.clone(),
            access_key_id: "********",
            secret_access_key: "********",
            session_token: "********",
            endpoint_url: config.endpoint_url.clone(),
            poll_interval_ms: config.poll_interval_ms,
            max_batch_size: config.max_batch_size,
            visibility_timeout_secs: config.visibility_timeout_secs,
            micro_batch_size: config.micro_batch_size,
            micro_batch_timeout_ms: config.micro_batch_timeout_ms,
            color: config.color.clone(),
            created_at: config.created_at.clone(),
            updated_at: config.updated_at.clone(),
        }
    }

    fn config_to_credentials(config: &Self::Config) -> Self::Credentials {
        QueueConnectionCredentials {
            id: config.id.clone(),
            name: config.name.clone(),
            queue_url: config.queue_url.clone(),
            dlq_url: config.dlq_url.clone(),
            provider: config.provider.clone(),
            region: config.region.clone(),
            access_key_id: config.access_key_id.clone(),
            secret_access_key: config.secret_access_key.clone(),
            session_token: config.session_token.clone(),
            endpoint_url: config.endpoint_url.clone(),
        }
    }

    fn stored_to_fallback_safe(stored: &Self::Stored) -> Self::Safe {
        QueueConnectionSafe {
            id: stored.id.clone(),
            name: stored.name.clone(),
            queue_url: stored.queue_url.clone(),
            dlq_url: stored.dlq_url.clone(),
            provider: stored.provider.clone(),
            enabled: stored.enabled,
            region: stored.region.clone(),
            access_key_id: "********",
            secret_access_key: "********",
            session_token: "********",
            endpoint_url: stored.endpoint_url.clone(),
            poll_interval_ms: stored.poll_interval_ms,
            max_batch_size: stored.max_batch_size,
            visibility_timeout_secs: stored.visibility_timeout_secs,
            micro_batch_size: stored.micro_batch_size,
            micro_batch_timeout_ms: stored.micro_batch_timeout_ms,
            color: stored.color.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        }
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

        let updated = store
            .update("update-queue", &updated_input)
            .unwrap()
            .unwrap();
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
