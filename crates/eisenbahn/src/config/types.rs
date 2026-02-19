use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Top-level config ────────────────────────────────────────────────

/// Full configuration for the eisenbahn messaging layer.
///
/// Parsed from `eisenbahn.toml` with support for environment variable overrides.
/// Defines the broker topology, worker processes, pipeline stages, and transport defaults.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EisenbahnConfig {
    /// Message broker (PUB/SUB) configuration.
    pub broker: BrokerConfig,

    /// Named worker processes that connect to the broker.
    #[serde(default)]
    pub workers: HashMap<String, WorkerConfig>,

    /// Pipeline stage definitions (PUSH/PULL work distribution).
    #[serde(default)]
    pub pipeline: PipelineTopology,

    /// Default transport settings.
    #[serde(default)]
    pub transport: TransportConfig,

    /// Named service endpoints for request/reply routing.
    #[serde(default)]
    pub services: HashMap<String, ServiceConfig>,
}

// ── Section configs ─────────────────────────────────────────────────

/// Broker section: the central PUB/SUB message hub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrokerConfig {
    /// Endpoint where publishers send messages (XSUB socket).
    #[serde(default = "default_broker_frontend")]
    pub frontend: String,

    /// Endpoint where subscribers receive messages (XPUB socket).
    #[serde(default = "default_broker_backend")]
    pub backend: String,

    /// Optional metrics/health endpoint port.
    pub metrics_port: Option<u16>,
}

fn default_broker_frontend() -> String {
    "ipc:///tmp/stupid-db/broker-frontend.sock".into()
}

fn default_broker_backend() -> String {
    "ipc:///tmp/stupid-db/broker-backend.sock".into()
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self {
            frontend: default_broker_frontend(),
            backend: default_broker_backend(),
            metrics_port: None,
        }
    }
}

/// Worker configuration: a named process that subscribes to events and connects to pipelines.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfig {
    /// Path to the worker binary (or crate name for cargo-run).
    pub binary: String,

    /// Event topics this worker subscribes to (e.g. `["entity.created", "anomaly.*"]`).
    #[serde(default)]
    pub subscriptions: Vec<String>,

    /// Pipeline stages this worker connects to (as sender or receiver).
    #[serde(default)]
    pub pipelines: Vec<String>,

    /// Number of worker instances to spawn (for horizontal scaling).
    #[serde(default = "default_instances")]
    pub instances: u32,

    /// Optional environment variables passed to the worker process.
    #[serde(default)]
    pub env: HashMap<String, String>,
}

fn default_instances() -> u32 {
    1
}

/// Pipeline topology: ordered stages for PUSH/PULL work distribution.
///
/// Not to be confused with `pipeline::PipelineConfig` which configures
/// ZMQ socket behavior (HWM, batch size). This struct defines the DAG
/// of processing stages.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineTopology {
    /// Named pipeline stages. Each stage defines its inputs (upstream stages).
    #[serde(default)]
    pub stages: HashMap<String, StageConfig>,
}

/// A single pipeline stage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StageConfig {
    /// Upstream stages that feed into this one. Empty = entry point.
    #[serde(default)]
    pub after: Vec<String>,

    /// Transport endpoint override for this stage's PUSH/PULL socket.
    pub endpoint: Option<String>,

    /// Number of parallel workers pulling from this stage.
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
}

fn default_concurrency() -> u32 {
    1
}

/// Transport defaults section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransportConfig {
    /// Default transport type: "ipc" or "tcp".
    #[serde(default = "default_transport_kind")]
    pub kind: String,

    /// Default TCP host (used when kind = "tcp").
    #[serde(default = "default_tcp_host")]
    pub default_host: String,

    /// Base port for auto-assigned TCP endpoints.
    #[serde(default = "default_base_port")]
    pub base_port: u16,
}

fn default_transport_kind() -> String {
    "ipc".into()
}

fn default_tcp_host() -> String {
    "127.0.0.1".into()
}

fn default_base_port() -> u16 {
    5560
}

impl Default for TransportConfig {
    fn default() -> Self {
        Self {
            kind: default_transport_kind(),
            default_host: default_tcp_host(),
            base_port: default_base_port(),
        }
    }
}

/// Configuration for a named request/reply service endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceConfig {
    /// The endpoint where the service worker binds its ROUTER socket.
    pub endpoint: String,

    /// Request timeout in seconds (default: 30).
    #[serde(default = "default_service_timeout")]
    pub timeout_secs: u64,
}

fn default_service_timeout() -> u64 {
    30
}
