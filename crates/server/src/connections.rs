//! Connection credential storage with AES-256-GCM encryption at rest.
//!
//! Stores database connection configs in `{DATA_DIR}/connections.json` with
//! passwords encrypted using AES-256-GCM. The encryption key is derived from
//! either the `DB_ENCRYPTION_KEY` env var or auto-generated in `{DATA_DIR}/.conn_key`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::credential_store::{
    self, encrypt_password, load_or_generate_key, slugify, CredentialStore,
};

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
fn parse_connection_string(
    url: &str,
) -> anyhow::Result<(String, u16, String, String, String, bool)> {
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
    let password = parsed
        .password()
        .map(|p| {
            urlencoding::decode(p)
                .unwrap_or_else(|_| p.into())
                .to_string()
        })
        .unwrap_or_default();

    // Check for SSL in query params
    let ssl = parsed.query_pairs().any(|(k, v)| {
        k == "sslmode" && (v == "require" || v == "verify-ca" || v == "verify-full")
    });

    if database.is_empty() {
        anyhow::bail!(
            "Connection string must include a database name (e.g. postgres://host/mydb)"
        );
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
pub(crate) struct StoredConnection {
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
}

impl CredentialStore for ConnectionStore {
    type Config = ConnectionConfig;
    type Safe = ConnectionSafe;
    type Credentials = ConnectionCredentials;
    type Input = ConnectionInput;
    type Stored = StoredConnection;

    fn store_path(&self) -> PathBuf {
        self.data_dir.join("connections.json")
    }

    fn type_name() -> &'static str {
        "connection"
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

    fn decrypt_record(&self, stored: &Self::Stored) -> anyhow::Result<Self::Config> {
        let password = credential_store::decrypt_password(&self.key, &stored.encrypted_password)?;
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

    fn config_to_safe(config: &Self::Config) -> Self::Safe {
        ConnectionSafe {
            id: config.id.clone(),
            name: config.name.clone(),
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            password: "********",
            ssl: config.ssl,
            color: config.color.clone(),
            created_at: config.created_at.clone(),
            updated_at: config.updated_at.clone(),
        }
    }

    fn config_to_credentials(config: &Self::Config) -> Self::Credentials {
        ConnectionCredentials {
            id: config.id.clone(),
            name: config.name.clone(),
            host: config.host.clone(),
            port: config.port,
            database: config.database.clone(),
            username: config.username.clone(),
            password: config.password.clone(),
            ssl: config.ssl,
        }
    }

    fn stored_to_fallback_safe(stored: &Self::Stored) -> Self::Safe {
        ConnectionSafe {
            id: stored.id.clone(),
            name: stored.name.clone(),
            host: stored.host.clone(),
            port: stored.port,
            database: stored.database.clone(),
            username: stored.username.clone(),
            password: "********",
            ssl: stored.ssl,
            color: stored.color.clone(),
            created_at: stored.created_at.clone(),
            updated_at: stored.updated_at.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            connection_string: Some(
                "postgresql://myuser:mypass@db.example.com:5433/mydb?sslmode=require".to_string(),
            ),
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
            connection_string: Some(
                "postgresql://admin:s3cret@prod.db.com:5433/analytics?sslmode=require".to_string(),
            ),
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
