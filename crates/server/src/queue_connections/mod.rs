//! SQS queue connection storage with AES-256-GCM encryption at rest.
//!
//! Stores queue connection configs in `{DATA_DIR}/queue-connections.json` with
//! credentials encrypted using AES-256-GCM. Implements [`CredentialStore`] for
//! shared CRUD logic.

mod store;
mod types;

#[cfg(test)]
mod tests;

pub use store::QueueConnectionStore;
pub use types::{
    QueueConnectionConfig, QueueConnectionCredentials, QueueConnectionInput, QueueConnectionSafe,
};
