//! AWS Athena connection storage with AES-256-GCM encryption at rest.
//!
//! Stores Athena connection configs in `{DATA_DIR}/athena-connections.json` with
//! credentials encrypted using AES-256-GCM. Implements [`CredentialStore`] for
//! shared CRUD logic, with extension methods for schema management.

mod store;
mod types;

#[cfg(test)]
mod tests;

pub use store::AthenaConnectionStore;
pub use types::{
    AthenaColumn, AthenaConnectionConfig, AthenaConnectionCredentials, AthenaConnectionInput,
    AthenaConnectionSafe, AthenaDatabase, AthenaSchema, AthenaTable,
};
