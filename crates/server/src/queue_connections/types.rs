//! Queue connection type definitions: Config, Safe, Credentials, Input, and Stored.

use serde::{Deserialize, Serialize};

// ── Default value functions ──────────────────────────────────────────

pub(super) fn default_provider() -> String {
    "sqs".to_string()
}

pub(super) fn default_enabled() -> bool {
    true
}

pub(super) fn default_region() -> String {
    "ap-southeast-1".to_string()
}

pub(super) fn default_poll_interval_ms() -> u64 {
    1000
}

pub(super) fn default_max_batch_size() -> u32 {
    10
}

pub(super) fn default_visibility_timeout_secs() -> u32 {
    30
}

pub(super) fn default_micro_batch_size() -> usize {
    100
}

pub(super) fn default_micro_batch_timeout_ms() -> u64 {
    1000
}

pub(super) fn default_color() -> String {
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
    pub(super) id: String,
    pub(super) name: String,
    pub(super) queue_url: String,
    pub(super) dlq_url: Option<String>,
    pub(super) provider: String,
    pub(super) enabled: bool,
    pub(super) region: String,
    pub(super) encrypted_access_key_id: String,
    pub(super) encrypted_secret_access_key: String,
    pub(super) encrypted_session_token: String,
    pub(super) endpoint_url: Option<String>,
    pub(super) poll_interval_ms: u64,
    pub(super) max_batch_size: u32,
    pub(super) visibility_timeout_secs: u32,
    pub(super) micro_batch_size: usize,
    pub(super) micro_batch_timeout_ms: u64,
    pub(super) color: String,
    pub(super) created_at: String,
    pub(super) updated_at: String,
}
