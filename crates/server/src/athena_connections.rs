//! AWS Athena connection storage with AES-256-GCM encryption at rest.
//!
//! Stores Athena connection configs in `{DATA_DIR}/athena-connections.json` with
//! credentials encrypted using AES-256-GCM. Implements [`CredentialStore`] for
//! shared CRUD logic, with extension methods for schema management.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tracing::info;

use crate::credential_store::{
    decrypt_password, encrypt_password, load_or_generate_key, slugify, CredentialStore,
};

// ── Default value functions ──────────────────────────────────────────

fn default_enabled() -> bool {
    true
}

fn default_region() -> String {
    "ap-southeast-1".to_string()
}

fn default_catalog() -> String {
    "AwsDataCatalog".to_string()
}

fn default_workgroup() -> String {
    "primary".to_string()
}

fn default_color() -> String {
    "#10b981".to_string()
}

fn default_schema_status() -> String {
    "pending".to_string()
}

// ── Schema types ─────────────────────────────────────────────────────

/// Cached Athena schema metadata (databases, tables, columns).
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AthenaSchema {
    pub databases: Vec<AthenaDatabase>,
    pub fetched_at: String,
}

/// A single Athena/Glue database with its tables.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AthenaDatabase {
    pub name: String,
    pub tables: Vec<AthenaTable>,
}

/// A single Athena table with its columns.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AthenaTable {
    pub name: String,
    pub columns: Vec<AthenaColumn>,
}

/// A single column in an Athena table.
#[derive(Debug, Clone, Serialize, Deserialize, utoipa::ToSchema)]
pub struct AthenaColumn {
    pub name: String,
    pub data_type: String,
    pub comment: Option<String>,
}

// ── Public types ─────────────────────────────────────────────────────

/// Full Athena connection config with decrypted credentials (internal use only).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AthenaConnectionConfig {
    pub id: String,
    pub name: String,
    pub region: String,
    pub catalog: String,
    pub database: String,
    pub workgroup: String,
    pub output_location: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub endpoint_url: Option<String>,
    pub enabled: bool,
    pub color: String,
    pub schema: Option<AthenaSchema>,
    pub schema_status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// JSON-safe version with masked credentials (returned by list/get endpoints).
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AthenaConnectionSafe {
    pub id: String,
    pub name: String,
    pub region: String,
    pub catalog: String,
    pub database: String,
    pub workgroup: String,
    pub output_location: String,
    #[schema(value_type = String)]
    pub access_key_id: &'static str,
    #[schema(value_type = String)]
    pub secret_access_key: &'static str,
    #[schema(value_type = String)]
    pub session_token: &'static str,
    pub endpoint_url: Option<String>,
    pub enabled: bool,
    pub color: String,
    pub schema: Option<AthenaSchema>,
    pub schema_status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Decrypted credentials for Athena client creation.
#[derive(Debug, Clone, Serialize, utoipa::ToSchema)]
pub struct AthenaConnectionCredentials {
    pub id: String,
    pub name: String,
    pub region: String,
    pub catalog: String,
    pub database: String,
    pub workgroup: String,
    pub output_location: String,
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub endpoint_url: Option<String>,
}

/// User input for creating/updating an Athena connection.
#[derive(Debug, Clone, Deserialize, utoipa::ToSchema)]
pub struct AthenaConnectionInput {
    pub name: String,
    #[serde(default = "default_region")]
    pub region: String,
    #[serde(default = "default_catalog")]
    pub catalog: String,
    pub database: String,
    #[serde(default = "default_workgroup")]
    pub workgroup: String,
    pub output_location: String,
    #[serde(default)]
    pub access_key_id: String,
    #[serde(default)]
    pub secret_access_key: String,
    #[serde(default)]
    pub session_token: String,
    #[serde(default)]
    pub endpoint_url: Option<String>,
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_color")]
    pub color: String,
}

// ── On-disk format ───────────────────────────────────────────────────

/// On-disk format: credentials stored as encrypted hex strings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct StoredAthenaConnection {
    id: String,
    name: String,
    region: String,
    catalog: String,
    database: String,
    workgroup: String,
    output_location: String,
    encrypted_access_key_id: String,
    encrypted_secret_access_key: String,
    encrypted_session_token: String,
    endpoint_url: Option<String>,
    enabled: bool,
    color: String,
    schema: Option<AthenaSchema>,
    schema_status: String,
    created_at: String,
    updated_at: String,
}

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

#[cfg(test)]
mod tests {
    use super::*;

    fn make_input(name: &str) -> AthenaConnectionInput {
        AthenaConnectionInput {
            name: name.to_string(),
            region: default_region(),
            catalog: default_catalog(),
            database: "analytics_db".to_string(),
            workgroup: default_workgroup(),
            output_location: "s3://my-athena-results/output/".to_string(),
            access_key_id: "AKIAIOSFODNN7EXAMPLE".to_string(),
            secret_access_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".to_string(),
            session_token: "FwoGZXIvYXdzEBYaDH7example".to_string(),
            endpoint_url: None,
            enabled: default_enabled(),
            color: default_color(),
        }
    }

    #[test]
    fn test_add_and_list() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Production Athena");
        let safe = store.add(&input).unwrap();

        assert_eq!(safe.id, "production-athena");
        assert_eq!(safe.access_key_id, "********");
        assert_eq!(safe.secret_access_key, "********");
        assert_eq!(safe.session_token, "********");
        assert_eq!(safe.database, "analytics_db");
        assert_eq!(safe.output_location, "s3://my-athena-results/output/");
        assert_eq!(safe.schema_status, "pending");
        assert!(safe.schema.is_none());

        let list = store.list().unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, "Production Athena");
        assert_eq!(list[0].access_key_id, "********");
    }

    #[test]
    fn test_get_credentials() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Creds Athena");
        store.add(&input).unwrap();

        let creds = store.get_credentials("creds-athena").unwrap().unwrap();
        assert_eq!(creds.access_key_id, "AKIAIOSFODNN7EXAMPLE");
        assert_eq!(
            creds.secret_access_key,
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        );
        assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
        assert_eq!(creds.database, "analytics_db");
        assert_eq!(creds.region, "ap-southeast-1");
        assert_eq!(creds.catalog, "AwsDataCatalog");
        assert_eq!(creds.workgroup, "primary");
    }

    #[test]
    fn test_update() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Update Athena");
        store.add(&input).unwrap();

        let mut updated_input = make_input("Update Athena");
        updated_input.database = "new_analytics_db".to_string();
        updated_input.access_key_id = "NEWKEY123".to_string();

        // Empty credentials should preserve the originals.
        updated_input.secret_access_key = String::new();
        updated_input.session_token = String::new();

        let updated = store
            .update("update-athena", &updated_input)
            .unwrap()
            .unwrap();
        assert_eq!(updated.database, "new_analytics_db");

        let creds = store
            .get_credentials("update-athena")
            .unwrap()
            .unwrap();
        assert_eq!(creds.database, "new_analytics_db");
        assert_eq!(creds.access_key_id, "NEWKEY123");
        // Preserved from original.
        assert_eq!(
            creds.secret_access_key,
            "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY"
        );
        assert_eq!(creds.session_token, "FwoGZXIvYXdzEBYaDH7example");
    }

    #[test]
    fn test_delete() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Delete Athena");
        store.add(&input).unwrap();

        assert!(store.delete("delete-athena").unwrap());
        assert!(!store.delete("delete-athena").unwrap());
        assert!(store.list().unwrap().is_empty());
    }

    #[test]
    fn test_duplicate_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Dupe Athena");
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

    #[test]
    fn test_schema_update() {
        let tmp = tempfile::tempdir().unwrap();
        let store = AthenaConnectionStore::new(&tmp.path().to_path_buf()).unwrap();

        let input = make_input("Schema Athena");
        store.add(&input).unwrap();

        // Verify initial status.
        let config = store.get("schema-athena").unwrap().unwrap();
        assert_eq!(config.schema_status, "pending");
        assert!(config.schema.is_none());

        // Update status to "fetching".
        assert!(store
            .update_schema_status("schema-athena", "fetching")
            .unwrap());
        let config = store.get("schema-athena").unwrap().unwrap();
        assert_eq!(config.schema_status, "fetching");

        // Set full schema.
        let schema = AthenaSchema {
            databases: vec![AthenaDatabase {
                name: "analytics_db".to_string(),
                tables: vec![AthenaTable {
                    name: "events".to_string(),
                    columns: vec![
                        AthenaColumn {
                            name: "event_id".to_string(),
                            data_type: "string".to_string(),
                            comment: Some("Unique event identifier".to_string()),
                        },
                        AthenaColumn {
                            name: "timestamp".to_string(),
                            data_type: "timestamp".to_string(),
                            comment: None,
                        },
                    ],
                }],
            }],
            fetched_at: chrono::Utc::now().to_rfc3339(),
        };

        assert!(store.update_schema("schema-athena", schema).unwrap());

        let config = store.get("schema-athena").unwrap().unwrap();
        assert_eq!(config.schema_status, "ready");
        let schema = config.schema.unwrap();
        assert_eq!(schema.databases.len(), 1);
        assert_eq!(schema.databases[0].name, "analytics_db");
        assert_eq!(schema.databases[0].tables.len(), 1);
        assert_eq!(schema.databases[0].tables[0].name, "events");
        assert_eq!(schema.databases[0].tables[0].columns.len(), 2);

        // Non-existent ID returns false.
        assert!(!store
            .update_schema_status("nonexistent", "failed")
            .unwrap());
    }
}
