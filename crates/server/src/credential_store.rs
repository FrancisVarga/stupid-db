//! Generic credential store trait for encrypted JSON-file-backed CRUD.
//!
//! Provides a [`CredentialStore`] trait with associated types and default method
//! implementations for list/get/add/update/delete operations. Shared encryption
//! helpers (`encrypt_password`, `decrypt_password`, `load_or_generate_key`) and
//! `slugify` are also defined here for reuse across all connection store modules.

use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use serde::{de::DeserializeOwned, Serialize};
use tracing::{info, warn};

// ── Encryption helpers ────────────────────────────────────────────

/// Encrypt a password using AES-256-GCM. Returns "iv:tag:ciphertext" in hex.
pub(crate) fn encrypt_password(key: &[u8; 32], plaintext: &str) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;

    let mut iv_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut iv_bytes);
    let nonce = Nonce::from_slice(&iv_bytes);

    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;

    // AES-GCM appends the 16-byte tag to the ciphertext.
    // Split into (ciphertext_without_tag, tag) for storage clarity.
    let tag_offset = ciphertext.len() - 16;
    let ct = &ciphertext[..tag_offset];
    let tag = &ciphertext[tag_offset..];

    Ok(format!(
        "{}:{}:{}",
        hex::encode(iv_bytes),
        hex::encode(tag),
        hex::encode(ct)
    ))
}

/// Decrypt a password from "iv:tag:ciphertext" hex format.
pub(crate) fn decrypt_password(key: &[u8; 32], encrypted: &str) -> anyhow::Result<String> {
    let parts: Vec<&str> = encrypted.splitn(3, ':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid encrypted password format (expected iv:tag:ciphertext)");
    }

    let iv_bytes = hex::decode(parts[0])?;
    let tag_bytes = hex::decode(parts[1])?;
    let ct_bytes = hex::decode(parts[2])?;

    if iv_bytes.len() != 12 {
        anyhow::bail!("Invalid IV length: expected 12, got {}", iv_bytes.len());
    }

    let cipher = Aes256Gcm::new_from_slice(key)
        .map_err(|e| anyhow::anyhow!("Failed to create cipher: {}", e))?;
    let nonce = Nonce::from_slice(&iv_bytes);

    // Reconstruct the ciphertext+tag as AES-GCM expects.
    let mut combined = ct_bytes;
    combined.extend_from_slice(&tag_bytes);

    let plaintext = cipher
        .decrypt(nonce, combined.as_ref())
        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;

    Ok(String::from_utf8(plaintext)?)
}

// ── Key management ────────────────────────────────────────────────

/// Load encryption key from `DB_ENCRYPTION_KEY` env var or auto-generate
/// in `{data_dir}/.conn_key`.
pub(crate) fn load_or_generate_key(data_dir: &PathBuf) -> anyhow::Result<[u8; 32]> {
    // Check env var first.
    if let Ok(env_key) = std::env::var("DB_ENCRYPTION_KEY") {
        let key_bytes = hex::decode(env_key.trim())?;
        if key_bytes.len() != 32 {
            anyhow::bail!(
                "DB_ENCRYPTION_KEY must be 64 hex characters (32 bytes), got {} bytes",
                key_bytes.len()
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        info!("Using encryption key from DB_ENCRYPTION_KEY env var");
        return Ok(key);
    }

    // Auto-generate key file.
    let key_path = data_dir.join(".conn_key");
    if key_path.exists() {
        let hex_key = std::fs::read_to_string(&key_path)?;
        let key_bytes = hex::decode(hex_key.trim())?;
        if key_bytes.len() != 32 {
            anyhow::bail!(
                "Invalid key file at {}: expected 32 bytes, got {}",
                key_path.display(),
                key_bytes.len()
            );
        }
        let mut key = [0u8; 32];
        key.copy_from_slice(&key_bytes);
        info!("Loaded encryption key from {}", key_path.display());
        return Ok(key);
    }

    // Generate new key.
    let mut key = [0u8; 32];
    OsRng.fill_bytes(&mut key);
    std::fs::create_dir_all(data_dir)?;
    std::fs::write(&key_path, hex::encode(key))?;
    info!("Generated new encryption key at {}", key_path.display());
    Ok(key)
}

// ── Slug generation ──────────────────────────────────────────────

/// Generate an ID from name: lowercase, replace non-alphanumeric with `-`,
/// collapse consecutive dashes, trim leading/trailing dashes.
pub(crate) fn slugify(name: &str) -> String {
    let slug: String = name
        .to_lowercase()
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();

    // Collapse consecutive dashes and trim.
    let mut result = String::new();
    let mut last_was_dash = false;
    for c in slug.chars() {
        if c == '-' {
            if !last_was_dash && !result.is_empty() {
                result.push('-');
            }
            last_was_dash = true;
        } else {
            result.push(c);
            last_was_dash = false;
        }
    }

    // Trim trailing dash.
    if result.ends_with('-') {
        result.pop();
    }

    if result.is_empty() {
        // Fallback for names that are entirely non-alphanumeric.
        format!("conn-{}", chrono::Utc::now().timestamp_millis())
    } else {
        result
    }
}

// ── CredentialStore trait ──────────────────────────────────────────

/// Generic trait for encrypted JSON-file-backed credential stores.
///
/// Implementors define five associated types forming a three-tier type system:
/// - `Config`: full internal config with decrypted secrets
/// - `Safe`: API-facing version with masked secrets
/// - `Credentials`: decrypted secrets for downstream consumers
/// - `Input`: creation/update payload from the API
/// - `Stored`: on-disk encrypted form (serialized to JSON)
///
/// Default method implementations handle all CRUD operations; implementors
/// only need to provide the type-specific encrypt/decrypt/conversion logic.
pub(crate) trait CredentialStore {
    type Config: Clone;
    type Safe: Clone + Serialize;
    type Credentials: Clone + Serialize;
    type Input;
    type Stored: Serialize + DeserializeOwned + Clone;

    /// Path to the JSON file backing this store.
    fn store_path(&self) -> PathBuf;

    /// Human-readable type name for log messages (e.g. "connection", "queue connection").
    fn type_name() -> &'static str;

    /// Generate a slug ID from the input (typically from the name field).
    fn generate_id(input: &Self::Input) -> String;

    /// Get the ID field from a stored record.
    fn stored_id(stored: &Self::Stored) -> &str;

    /// Get the created_at field from a stored record.
    fn stored_created_at(stored: &Self::Stored) -> &str;

    /// Encrypt an input into a stored record (for `add`).
    fn encrypt_record(
        &self,
        id: &str,
        input: &Self::Input,
        created_at: &str,
        updated_at: &str,
    ) -> anyhow::Result<Self::Stored>;

    /// Encrypt an input into a stored record for `update`, with access to the
    /// existing record for credential preservation.
    ///
    /// Default: delegates to [`encrypt_record`](Self::encrypt_record) (no preservation).
    fn encrypt_record_update(
        &self,
        id: &str,
        input: &Self::Input,
        _existing: &Self::Stored,
        created_at: &str,
        updated_at: &str,
    ) -> anyhow::Result<Self::Stored> {
        self.encrypt_record(id, input, created_at, updated_at)
    }

    /// Decrypt a stored record into a full config.
    fn decrypt_record(&self, stored: &Self::Stored) -> anyhow::Result<Self::Config>;

    /// Convert a full config to a safe (masked) version.
    fn config_to_safe(config: &Self::Config) -> Self::Safe;

    /// Convert a full config to a credentials-only version.
    fn config_to_credentials(config: &Self::Config) -> Self::Credentials;

    /// Build a safe fallback directly from a stored record when decryption fails.
    fn stored_to_fallback_safe(stored: &Self::Stored) -> Self::Safe;

    // ── Default CRUD implementations ──────────────────────────────

    /// Load all stored records from the JSON file.
    fn load_stored(&self) -> anyhow::Result<Vec<Self::Stored>> {
        let path = self.store_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let records: Vec<Self::Stored> = serde_json::from_str(&data)?;
        Ok(records)
    }

    /// Save all stored records to the JSON file.
    fn save_stored(&self, records: &[Self::Stored]) -> anyhow::Result<()> {
        let path = self.store_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(records)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    /// List all records with masked secrets.
    fn list(&self) -> anyhow::Result<Vec<Self::Safe>> {
        let stored = self.load_stored()?;
        let mut result = Vec::with_capacity(stored.len());
        for s in &stored {
            match self.decrypt_record(s) {
                Ok(c) => result.push(Self::config_to_safe(&c)),
                Err(e) => {
                    warn!(
                        "Failed to decrypt {} '{}': {}",
                        Self::type_name(),
                        Self::stored_id(s),
                        e
                    );
                    result.push(Self::stored_to_fallback_safe(s));
                }
            }
        }
        Ok(result)
    }

    /// Get a single record by ID with masked secrets.
    fn get_safe(&self, id: &str) -> anyhow::Result<Option<Self::Safe>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| Self::stored_id(s) == id) {
            Some(s) => {
                let c = self.decrypt_record(s)?;
                Ok(Some(Self::config_to_safe(&c)))
            }
            None => Ok(None),
        }
    }

    /// Get a single record by ID with decrypted secrets (internal use).
    fn get(&self, id: &str) -> anyhow::Result<Option<Self::Config>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| Self::stored_id(s) == id) {
            Some(s) => Ok(Some(self.decrypt_record(s)?)),
            None => Ok(None),
        }
    }

    /// Get decrypted credentials for downstream consumers.
    fn get_credentials(&self, id: &str) -> anyhow::Result<Option<Self::Credentials>> {
        self.get(id)
            .map(|opt| opt.map(|c| Self::config_to_credentials(&c)))
    }

    /// Add a new record. Returns the created record (safe).
    fn add(&self, input: &Self::Input) -> anyhow::Result<Self::Safe> {
        let mut stored = self.load_stored()?;
        let id = Self::generate_id(input);

        if stored.iter().any(|s| Self::stored_id(s) == id) {
            anyhow::bail!(
                "{} with id '{}' already exists",
                Self::type_name(),
                id
            );
        }

        let now = chrono::Utc::now().to_rfc3339();
        let record = self.encrypt_record(&id, input, &now, &now)?;
        stored.push(record.clone());
        self.save_stored(&stored)?;

        let config = self.decrypt_record(&record)?;
        info!("Added {} '{}'", Self::type_name(), id);
        Ok(Self::config_to_safe(&config))
    }

    /// Update an existing record. Returns the updated record (safe), or None if not found.
    fn update(&self, id: &str, input: &Self::Input) -> anyhow::Result<Option<Self::Safe>> {
        let mut stored = self.load_stored()?;
        let idx = match stored.iter().position(|s| Self::stored_id(s) == id) {
            Some(i) => i,
            None => return Ok(None),
        };

        let created_at = Self::stored_created_at(&stored[idx]).to_string();
        let now = chrono::Utc::now().to_rfc3339();
        let record = self.encrypt_record_update(id, input, &stored[idx], &created_at, &now)?;
        stored[idx] = record.clone();
        self.save_stored(&stored)?;

        let config = self.decrypt_record(&record)?;
        info!("Updated {} '{}'", Self::type_name(), id);
        Ok(Some(Self::config_to_safe(&config)))
    }

    /// Delete a record by ID. Returns true if it existed.
    fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut stored = self.load_stored()?;
        let len_before = stored.len();
        stored.retain(|s| Self::stored_id(s) != id);
        if stored.len() == len_before {
            return Ok(false);
        }
        self.save_stored(&stored)?;
        info!("Deleted {} '{}'", Self::type_name(), id);
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_slugify() {
        assert_eq!(slugify("My Database"), "my-database");
        assert_eq!(slugify("Production (Primary)"), "production-primary");
        assert_eq!(slugify("dev-server-01"), "dev-server-01");
        assert_eq!(slugify("  spaces  "), "spaces");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);

        let password = "super_secret_password_123!";
        let encrypted = encrypt_password(&key, password).unwrap();
        let decrypted = decrypt_password(&key, &encrypted).unwrap();
        assert_eq!(decrypted, password);
    }

    #[test]
    fn test_encrypt_format() {
        let mut key = [0u8; 32];
        OsRng.fill_bytes(&mut key);

        let encrypted = encrypt_password(&key, "test").unwrap();
        let parts: Vec<&str> = encrypted.splitn(3, ':').collect();
        assert_eq!(parts.len(), 3);
        // IV = 12 bytes = 24 hex chars
        assert_eq!(parts[0].len(), 24);
        // Tag = 16 bytes = 32 hex chars
        assert_eq!(parts[1].len(), 32);
    }
}
