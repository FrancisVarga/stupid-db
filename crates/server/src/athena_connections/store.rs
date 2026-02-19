//! [`AthenaConnectionStore`] — thread-safe credential store with AES-256-GCM encryption
//! and [`CredentialStore`] trait implementation.

use std::path::PathBuf;

use tracing::info;

use crate::credential_store::{
    decrypt_password, encrypt_password, load_or_generate_key, slugify, CredentialStore,
};

use super::types::*;

// ── Store ────────────────────────────────────────────────────────────

/// Thread-safe Athena connection credential store.
pub struct AthenaConnectionStore {
    data_dir: PathBuf,
    key: [u8; 32],
}

impl AthenaConnectionStore {
    /// Create a new store, loading or generating the encryption key.
    pub fn new(data_dir: &PathBuf) -> anyhow::Result<Self> {
        let key = load_or_generate_key(data_dir)?;
        Ok(Self {
            data_dir: data_dir.clone(),
            key,
        })
    }

    // ── Athena-specific extension methods ─────────────────────────

    /// Update the cached schema for a connection, setting status to "ready".
    pub fn update_schema(&self, id: &str, schema: AthenaSchema) -> anyhow::Result<bool> {
        let mut stored = self.load_stored()?;
        let idx = match stored.iter().position(|s| s.id == id) {
            Some(i) => i,
            None => return Ok(false),
        };

        stored[idx].schema = Some(schema);
        stored[idx].schema_status = "ready".to_string();
        stored[idx].updated_at = chrono::Utc::now().to_rfc3339();
        self.save_stored(&stored)?;

        info!("Updated schema for Athena connection '{}'", id);
        Ok(true)
    }

    /// Update just the schema_status field for a connection.
    pub fn update_schema_status(&self, id: &str, status: &str) -> anyhow::Result<bool> {
        let mut stored = self.load_stored()?;
        let idx = match stored.iter().position(|s| s.id == id) {
            Some(i) => i,
            None => return Ok(false),
        };

        stored[idx].schema_status = status.to_string();
        stored[idx].updated_at = chrono::Utc::now().to_rfc3339();
        self.save_stored(&stored)?;

        info!(
            "Updated schema status for Athena connection '{}' to '{}'",
            id, status
        );
        Ok(true)
    }
}

impl CredentialStore for AthenaConnectionStore {
    type Config = AthenaConnectionConfig;
    type Safe = AthenaConnectionSafe;
    type Credentials = AthenaConnectionCredentials;
    type Input = AthenaConnectionInput;
    type Stored = StoredAthenaConnection;

    fn store_path(&self) -> PathBuf {
        self.data_dir.join("athena-connections.json")
    }

    fn type_name() -> &'static str {
        "Athena connection"
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

        Ok(StoredAthenaConnection {
            id: id.to_string(),
            name: input.name.clone(),
            region: input.region.clone(),
            catalog: input.catalog.clone(),
            database: input.database.clone(),
            workgroup: input.workgroup.clone(),
            output_location: input.output_location.clone(),
            encrypted_access_key_id,
            encrypted_secret_access_key,
            encrypted_session_token,
            endpoint_url: input.endpoint_url.clone(),
            enabled: input.enabled,
            color: input.color.clone(),
            schema: None,
            schema_status: default_schema_status(),
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

        // Preserve existing schema data across updates.
        Ok(StoredAthenaConnection {
            id: id.to_string(),
            name: input.name.clone(),
            region: input.region.clone(),
            catalog: input.catalog.clone(),
            database: input.database.clone(),
            workgroup: input.workgroup.clone(),
            output_location: input.output_location.clone(),
            encrypted_access_key_id,
            encrypted_secret_access_key,
            encrypted_session_token,
            endpoint_url: input.endpoint_url.clone(),
            enabled: input.enabled,
            color: input.color.clone(),
            schema: existing.schema.clone(),
            schema_status: existing.schema_status.clone(),
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        })
    }

    fn decrypt_record(&self, stored: &Self::Stored) -> anyhow::Result<Self::Config> {
        let access_key_id = decrypt_password(&self.key, &stored.encrypted_access_key_id)?;
        let secret_access_key =
            decrypt_password(&self.key, &stored.encrypted_secret_access_key)?;
        let session_token = decrypt_password(&self.key, &stored.encrypted_session_token)?;
        Ok(AthenaConnectionConfig {
            id: stored.id.clone(),
            name: stored.name.clone(),
            region: stored.region.clone(),
            catalog: stored.catalog.clone(),
            database: stored.database.clone(),
            workgroup: stored.workgroup.clone(),
            output_location: stored.output_location.clone(),
            access_key_id,
            secret_access_key,
            session_token,
            endpoint_url: stored.endpoint_url.clone(),
            enabled: stored.enabled,
            color: stored.color.clone(),
            schema: stored.schema.clone(),
            schema_status: stored.schema_status.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        })
    }

    fn config_to_safe(config: &Self::Config) -> Self::Safe {
        AthenaConnectionSafe {
            id: config.id.clone(),
            name: config.name.clone(),
            region: config.region.clone(),
            catalog: config.catalog.clone(),
            database: config.database.clone(),
            workgroup: config.workgroup.clone(),
            output_location: config.output_location.clone(),
            access_key_id: "********",
            secret_access_key: "********",
            session_token: "********",
            endpoint_url: config.endpoint_url.clone(),
            enabled: config.enabled,
            color: config.color.clone(),
            schema: config.schema.clone(),
            schema_status: config.schema_status.clone(),
            created_at: config.created_at.clone(),
            updated_at: config.updated_at.clone(),
        }
    }

    fn config_to_credentials(config: &Self::Config) -> Self::Credentials {
        AthenaConnectionCredentials {
            id: config.id.clone(),
            name: config.name.clone(),
            region: config.region.clone(),
            catalog: config.catalog.clone(),
            database: config.database.clone(),
            workgroup: config.workgroup.clone(),
            output_location: config.output_location.clone(),
            access_key_id: config.access_key_id.clone(),
            secret_access_key: config.secret_access_key.clone(),
            session_token: config.session_token.clone(),
            endpoint_url: config.endpoint_url.clone(),
        }
    }

    fn stored_to_fallback_safe(stored: &Self::Stored) -> Self::Safe {
        AthenaConnectionSafe {
            id: stored.id.clone(),
            name: stored.name.clone(),
            region: stored.region.clone(),
            catalog: stored.catalog.clone(),
            database: stored.database.clone(),
            workgroup: stored.workgroup.clone(),
            output_location: stored.output_location.clone(),
            access_key_id: "********",
            secret_access_key: "********",
            session_token: "********",
            endpoint_url: stored.endpoint_url.clone(),
            enabled: stored.enabled,
            color: stored.color.clone(),
            schema: stored.schema.clone(),
            schema_status: stored.schema_status.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        }
    }
}
