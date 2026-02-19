//! Type definitions for Athena connections: schema, config, safe, credentials, input, and stored formats.

use serde::{Deserialize, Serialize};

// ── Default value functions ──────────────────────────────────────────

pub(crate) fn default_enabled() -> bool {
    true
}

pub(crate) fn default_region() -> String {
    "ap-southeast-1".to_string()
}

pub(crate) fn default_catalog() -> String {
    "AwsDataCatalog".to_string()
}

pub(crate) fn default_workgroup() -> String {
    "primary".to_string()
}

pub(crate) fn default_color() -> String {
    "#10b981".to_string()
}

pub(crate) fn default_schema_status() -> String {
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
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) region: String,
    pub(crate) catalog: String,
    pub(crate) database: String,
    pub(crate) workgroup: String,
    pub(crate) output_location: String,
    pub(crate) encrypted_access_key_id: String,
    pub(crate) encrypted_secret_access_key: String,
    pub(crate) encrypted_session_token: String,
    pub(crate) endpoint_url: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) color: String,
    pub(crate) schema: Option<AthenaSchema>,
    pub(crate) schema_status: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}
