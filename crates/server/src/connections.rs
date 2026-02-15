//! Connection credential storage with AES-256-GCM encryption at rest.
//!
//! Stores database connection configs in `{DATA_DIR}/connections.json` with
//! passwords encrypted using AES-256-GCM. The encryption key is derived from
//! either the `DB_ENCRYPTION_KEY` env var or auto-generated in `{DATA_DIR}/.conn_key`.

use std::path::PathBuf;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

/// Full connection config with decrypted password (internal use only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

/// JSON-safe version with masked password (returned by list/get endpoints).
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionSafe {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: &'static str,
    pub ssl: bool,
    pub color: String,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&ConnectionConfig> for ConnectionSafe {
    fn from(c: &ConnectionConfig) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            host: c.host.clone(),
            port: c.port,
            database: c.database.clone(),
            username: c.username.clone(),
            password: "********",
            ssl: c.ssl,
            color: c.color.clone(),
            created_at: c.created_at.clone(),
            updated_at: c.updated_at.clone(),
        }
    }
}

/// Decrypted credentials returned by the `/credentials` endpoint.
#[derive(Debug, Clone, Serialize)]
pub struct ConnectionCredentials {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub database: String,
    pub username: String,
    pub password: String,
    pub ssl: bool,
}

impl From<&ConnectionConfig> for ConnectionCredentials {
    fn from(c: &ConnectionConfig) -> Self {
        Self {
            id: c.id.clone(),
            name: c.name.clone(),
            host: c.host.clone(),
            port: c.port,
            database: c.database.clone(),
            username: c.username.clone(),
            password: c.password.clone(),
            ssl: c.ssl,
        }
    }
}

/// User input for creating/updating a connection.
/// Accepts either individual fields OR a `connection_string`.
/// When `connection_string` is provided, it overrides host/port/database/username/password/ssl.
#[derive(Debug, Clone, Deserialize)]
pub struct ConnectionInput {
    pub name: String,
    /// Optional connection string: `postgresql://user:pass@host:port/dbname?sslmode=require`
    #[serde(default)]
    pub connection_string: Option<String>,
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default)]
    pub database: String,
    #[serde(default = "default_username")]
    pub username: String,
    #[serde(default)]
    pub password: String,
    #[serde(default)]
    pub ssl: bool,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "#3b82f6".to_string()
}

fn default_host() -> String {
    "localhost".to_string()
}

fn default_port() -> u16 {
    5432
}

fn default_username() -> String {
    "postgres".to_string()
}

/// Parse a PostgreSQL connection string into individual components.
/// Supports: `postgresql://user:pass@host:port/dbname?sslmode=require`
/// Also supports: `postgres://...`
fn parse_connection_string(url: &str) -> anyhow::Result<(String, u16, String, String, String, bool)> {
    let url = url.trim();

    // Basic URL parsing
    let parsed = url::Url::parse(url)
        .map_err(|e| anyhow::anyhow!("Invalid connection string: {}", e))?;

    if parsed.scheme() != "postgresql" && parsed.scheme() != "postgres" {
        anyhow::bail!("Connection string must start with postgresql:// or postgres://");
    }

    let host = parsed.host_str().unwrap_or("localhost").to_string();
    let port = parsed.port().unwrap_or(5432);
    let database = parsed.path().trim_start_matches('/').to_string();
    let username = if parsed.username().is_empty() {
        "postgres".to_string()
    } else {
        // URL-decode the username
        urlencoding::decode(parsed.username())
            .unwrap_or_else(|_| parsed.username().into())
            .to_string()
    };
    let password = parsed.password()
        .map(|p| urlencoding::decode(p).unwrap_or_else(|_| p.into()).to_string())
        .unwrap_or_default();

    // Check for SSL in query params
    let ssl = parsed.query_pairs().any(|(k, v)| {
        k == "sslmode" && (v == "require" || v == "verify-ca" || v == "verify-full")
    });

    if database.is_empty() {
        anyhow::bail!("Connection string must include a database name (e.g. postgres://host/mydb)");
    }

    Ok((host, port, database, username, password, ssl))
}

impl ConnectionInput {
    /// Resolve the final connection parameters.
    /// If `connection_string` is set, parse it and override individual fields.
    pub fn resolve(&self) -> anyhow::Result<(String, u16, String, String, String, bool)> {
        if let Some(ref cs) = self.connection_string {
            if !cs.trim().is_empty() {
                return parse_connection_string(cs);
            }
        }
        Ok((
            self.host.clone(),
            self.port,
            self.database.clone(),
            self.username.clone(),
            self.password.clone(),
            self.ssl,
        ))
    }
}

/// On-disk format: passwords stored as encrypted hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct StoredConnection {
    id: String,
    name: String,
    host: String,
    port: u16,
    database: String,
    username: String,
    encrypted_password: String, // iv:tag:ciphertext in hex
    ssl: bool,
    color: String,
    created_at: String,
    updated_at: String,
}

/// Thread-safe connection credential store.
pub struct ConnectionStore {
    data_dir: PathBuf,
    key: [u8; 32],
}

impl ConnectionStore {
    /// Create a new store, loading or generating the encryption key.
    pub fn new(data_dir: &PathBuf) -> anyhow::Result<Self> {
        let key = load_or_generate_key(data_dir)?;
        Ok(Self {
            data_dir: data_dir.clone(),
            key,
        })
    }

    fn connections_path(&self) -> PathBuf {
        self.data_dir.join("connections.json")
    }

    fn load_stored(&self) -> anyhow::Result<Vec<StoredConnection>> {
        let path = self.connections_path();
        if !path.exists() {
            return Ok(Vec::new());
        }
        let data = std::fs::read_to_string(&path)?;
        let connections: Vec<StoredConnection> = serde_json::from_str(&data)?;
        Ok(connections)
    }

    fn save_stored(&self, connections: &[StoredConnection]) -> anyhow::Result<()> {
        let path = self.connections_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(connections)?;
        std::fs::write(&path, data)?;
        Ok(())
    }

    fn decrypt_connection(&self, stored: &StoredConnection) -> anyhow::Result<ConnectionConfig> {
        let password = decrypt_password(&self.key, &stored.encrypted_password)?;
        Ok(ConnectionConfig {
            id: stored.id.clone(),
            name: stored.name.clone(),
            host: stored.host.clone(),
            port: stored.port,
            database: stored.database.clone(),
            username: stored.username.clone(),
            password,
            ssl: stored.ssl,
            color: stored.color.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        })
    }

    fn encrypt_connection(
        &self,
        id: &str,
        input: &ConnectionInput,
        created_at: &str,
        updated_at: &str,
    ) -> anyhow::Result<StoredConnection> {
        let (host, port, database, username, password, ssl) = input.resolve()?;
        let encrypted_password = encrypt_password(&self.key, &password)?;
        Ok(StoredConnection {
            id: id.to_string(),
            name: input.name.clone(),
            host,
            port,
            database,
            username,
            encrypted_password,
            ssl,
            color: input.color.clone(),
            created_at: created_at.to_string(),
            updated_at: updated_at.to_string(),
        })
    }

    /// List all connections with masked passwords.
    pub fn list(&self) -> anyhow::Result<Vec<ConnectionSafe>> {
        let stored = self.load_stored()?;
        let mut result = Vec::with_capacity(stored.len());
        for s in &stored {
            match self.decrypt_connection(s) {
                Ok(c) => result.push(ConnectionSafe::from(&c)),
                Err(e) => {
                    warn!("Failed to decrypt connection '{}': {}", s.id, e);
                    // Return a safe version with empty fields rather than skipping.
                    result.push(ConnectionSafe {
                        id: s.id.clone(),
                        name: s.name.clone(),
                        host: s.host.clone(),
                        port: s.port,
                        database: s.database.clone(),
                        username: s.username.clone(),
                        password: "********",
                        ssl: s.ssl,
                        color: s.color.clone(),
                        created_at: s.created_at.clone(),
                        updated_at: s.updated_at.clone(),
                    });
                }
            }
        }
        Ok(result)
    }

    /// Get a single connection by ID with masked password.
    pub fn get_safe(&self, id: &str) -> anyhow::Result<Option<ConnectionSafe>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| s.id == id) {
            Some(s) => {
                let c = self.decrypt_connection(s)?;
                Ok(Some(ConnectionSafe::from(&c)))
            }
            None => Ok(None),
        }
    }

    /// Get a single connection by ID with decrypted password (internal use).
    pub fn get(&self, id: &str) -> anyhow::Result<Option<ConnectionConfig>> {
        let stored = self.load_stored()?;
        match stored.iter().find(|s| s.id == id) {
            Some(s) => Ok(Some(self.decrypt_connection(s)?)),
            None => Ok(None),
        }
    }

    /// Get decrypted credentials for pool creation.
    pub fn get_credentials(&self, id: &str) -> anyhow::Result<Option<ConnectionCredentials>> {
        self.get(id).map(|opt| opt.map(|c| ConnectionCredentials::from(&c)))
    }

    /// Add a new connection. Returns the created connection (safe).
    pub fn add(&self, input: &ConnectionInput) -> anyhow::Result<ConnectionSafe> {
        let mut stored = self.load_stored()?;
        let id = slugify(&input.name);

        // Check for duplicate ID.
        if stored.iter().any(|s| s.id == id) {
            anyhow::bail!("Connection with id '{}' already exists", id);
        }

        let now = chrono::Utc::now().to_rfc3339();
        let encrypted = self.encrypt_connection(&id, input, &now, &now)?;
        stored.push(encrypted);
        self.save_stored(&stored)?;

        let (host, port, database, username, password, ssl) = input.resolve()?;
        info!("Added connection '{}' ({}:{})", id, host, port);
        let config = ConnectionConfig {
            id,
            name: input.name.clone(),
            host,
            port,
            database,
            username,
            password,
            ssl,
            color: input.color.clone(),
            created_at: now.clone(),
            updated_at: now,
        };
        Ok(ConnectionSafe::from(&config))
    }

    /// Update an existing connection. Returns the updated connection (safe).
    pub fn update(&self, id: &str, input: &ConnectionInput) -> anyhow::Result<Option<ConnectionSafe>> {
        let mut stored = self.load_stored()?;
        let idx = match stored.iter().position(|s| s.id == id) {
            Some(i) => i,
            None => return Ok(None),
        };

        let created_at = stored[idx].created_at.clone();
        let now = chrono::Utc::now().to_rfc3339();
        let encrypted = self.encrypt_connection(id, input, &created_at, &now)?;
        stored[idx] = encrypted;
        self.save_stored(&stored)?;

        let (host, port, database, username, password, ssl) = input.resolve()?;
        info!("Updated connection '{}'", id);
        let config = ConnectionConfig {
            id: id.to_string(),
            name: input.name.clone(),
            host,
            port,
            database,
            username,
            password,
            ssl,
            color: input.color.clone(),
            created_at,
            updated_at: now,
        };
        Ok(Some(ConnectionSafe::from(&config)))
    }

    /// Delete a connection by ID. Returns true if it existed.
    pub fn delete(&self, id: &str) -> anyhow::Result<bool> {
        let mut stored = self.load_stored()?;
        let len_before = stored.len();
        stored.retain(|s| s.id != id);
        if stored.len() == len_before {
            return Ok(false);
        }
        self.save_stored(&stored)?;
        info!("Deleted connection '{}'", id);
        Ok(true)
    }
}

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

    Ok(format!("{}:{}:{}", hex::encode(iv_bytes), hex::encode(tag), hex::encode(ct)))
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

    #[test]
    fn test_store_crud() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        // Add
        let input = ConnectionInput {
            name: "Test DB".to_string(),
            connection_string: None,
            host: "localhost".to_string(),
            port: 5432,
            database: "testdb".to_string(),
            username: "admin".to_string(),
            password: "secret123".to_string(),
            ssl: false,
            color: "#ff0000".to_string(),
        };

        let safe = store.add(&input).unwrap();
        assert_eq!(safe.id, "test-db");
        assert_eq!(safe.password, "********");

        // List
        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Test DB");

        // Get safe
        let got = store.get_safe("test-db").unwrap().unwrap();
        assert_eq!(got.host, "localhost");
        assert_eq!(got.password, "********");

        // Get decrypted
        let full = store.get("test-db").unwrap().unwrap();
        assert_eq!(full.password, "secret123");

        // Get credentials
        let creds = store.get_credentials("test-db").unwrap().unwrap();
        assert_eq!(creds.password, "secret123");
        assert_eq!(creds.host, "localhost");

        // Update
        let updated_input = ConnectionInput {
            name: "Test DB Updated".to_string(),
            connection_string: None,
            host: "db.example.com".to_string(),
            port: 5433,
            database: "testdb".to_string(),
            username: "admin".to_string(),
            password: "new_secret".to_string(),
            ssl: true,
            color: "#00ff00".to_string(),
        };

        let updated = store.update("test-db", &updated_input).unwrap().unwrap();
        assert_eq!(updated.host, "db.example.com");

        let full = store.get("test-db").unwrap().unwrap();
        assert_eq!(full.password, "new_secret");
        assert_eq!(full.ssl, true);

        // Delete
        assert!(store.delete("test-db").unwrap());
        assert!(!store.delete("test-db").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_connection_string_parsing() {
        let input = ConnectionInput {
            name: "From URL".to_string(),
            connection_string: Some("postgresql://myuser:mypass@db.example.com:5433/mydb?sslmode=require".to_string()),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            ssl: false,
            color: "#00f0ff".to_string(),
        };

        let (host, port, database, username, password, ssl) = input.resolve().unwrap();
        assert_eq!(host, "db.example.com");
        assert_eq!(port, 5433);
        assert_eq!(database, "mydb");
        assert_eq!(username, "myuser");
        assert_eq!(password, "mypass");
        assert!(ssl);
    }

    #[test]
    fn test_connection_string_defaults() {
        let input = ConnectionInput {
            name: "Minimal URL".to_string(),
            connection_string: Some("postgres://localhost/testdb".to_string()),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            ssl: false,
            color: "#00f0ff".to_string(),
        };

        let (host, port, database, username, password, ssl) = input.resolve().unwrap();
        assert_eq!(host, "localhost");
        assert_eq!(port, 5432);
        assert_eq!(database, "testdb");
        assert_eq!(username, "postgres");
        assert_eq!(password, "");
        assert!(!ssl);
    }

    #[test]
    fn test_connection_string_store_roundtrip() {
        let tmp = tempfile::tempdir().unwrap();
        let store = ConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = ConnectionInput {
            name: "URL DB".to_string(),
            connection_string: Some("postgresql://admin:s3cret@prod.db.com:5433/analytics?sslmode=require".to_string()),
            host: String::new(),
            port: 0,
            database: String::new(),
            username: String::new(),
            password: String::new(),
            ssl: false,
            color: "#a855f7".to_string(),
        };

        let safe = store.add(&input).unwrap();
        assert_eq!(safe.id, "url-db");
        assert_eq!(safe.host, "prod.db.com");
        assert_eq!(safe.port, 5433);
        assert_eq!(safe.database, "analytics");

        let creds = store.get_credentials("url-db").unwrap().unwrap();
        assert_eq!(creds.password, "s3cret");
        assert_eq!(creds.username, "admin");
        assert!(creds.ssl);
    }
}
