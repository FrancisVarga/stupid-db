//! [`QueueConnectionStore`] — thread-safe credential store with AES-256-GCM encryption.

use std::path::PathBuf;

use crate::credential_store::{
    decrypt_password, encrypt_password, load_or_generate_key, slugify, CredentialStore,
};

use super::types::{
    QueueConnectionConfig, QueueConnectionCredentials, QueueConnectionInput, QueueConnectionSafe,
    StoredQueueConnection,
};

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
