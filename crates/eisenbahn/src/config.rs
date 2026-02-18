use std::collections::{HashMap, VecDeque};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::EisenbahnError;
use crate::transport::Transport;

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

// ── Loading & Validation ────────────────────────────────────────────

impl EisenbahnConfig {
    /// Parse config from a TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Self, EisenbahnError> {
        let mut config: Self = toml::from_str(toml_str)?;
        config.apply_env_overrides();
        config.validate()?;
        Ok(config)
    }

    /// Load config from a file path.
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self, EisenbahnError> {
        let content = std::fs::read_to_string(path.as_ref())?;
        Self::from_toml(&content)
    }

    /// Create a config for single-host deployment using IPC sockets.
    pub fn local() -> Self {
        Self {
            broker: BrokerConfig::default(),
            workers: HashMap::new(),
            pipeline: PipelineTopology::default(),
            transport: TransportConfig::default(),
            services: HashMap::new(),
        }
    }

    /// Create a config for distributed deployment using TCP.
    pub fn distributed(broker_host: &str, broker_port: u16) -> Self {
        Self {
            broker: BrokerConfig {
                frontend: format!("tcp://{broker_host}:{broker_port}"),
                backend: format!("tcp://{broker_host}:{}", broker_port + 1),
                metrics_port: Some(broker_port + 2),
            },
            workers: HashMap::new(),
            pipeline: PipelineTopology::default(),
            transport: TransportConfig {
                kind: "tcp".into(),
                default_host: broker_host.into(),
                base_port: broker_port + 10,
            },
            services: HashMap::new(),
        }
    }

    /// Resolve the broker's frontend transport.
    pub fn broker_frontend_transport(&self) -> Transport {
        parse_endpoint_to_transport(&self.broker.frontend)
    }

    /// Resolve the broker's backend transport.
    pub fn broker_backend_transport(&self) -> Transport {
        parse_endpoint_to_transport(&self.broker.backend)
    }

    /// Resolve a named service's endpoint to a [`Transport`].
    ///
    /// Returns `None` if the service name is not configured.
    pub fn service_transport(&self, name: &str) -> Option<Transport> {
        self.services
            .get(name)
            .map(|svc| parse_endpoint_to_transport(&svc.endpoint))
    }

    /// Get the topologically sorted pipeline stage order.
    ///
    /// Returns stage names in execution order (upstream before downstream).
    pub fn pipeline_order(&self) -> Result<Vec<String>, EisenbahnError> {
        topological_sort(&self.pipeline.stages)
    }

    // ── Environment variable overrides ──────────────────────────────

    /// Apply environment variable overrides.
    ///
    /// Convention: `EISENBAHN_SECTION_KEY` overrides `section.key`.
    /// Examples:
    /// - `EISENBAHN_BROKER_FRONTEND` → `broker.frontend`
    /// - `EISENBAHN_BROKER_BACKEND` → `broker.backend`
    /// - `EISENBAHN_BROKER_METRICS_PORT` → `broker.metrics_port`
    /// - `EISENBAHN_TRANSPORT_KIND` → `transport.kind`
    /// - `EISENBAHN_TRANSPORT_DEFAULT_HOST` → `transport.default_host`
    /// - `EISENBAHN_TRANSPORT_BASE_PORT` → `transport.base_port`
    fn apply_env_overrides(&mut self) {
        if let Ok(v) = std::env::var("EISENBAHN_BROKER_FRONTEND") {
            self.broker.frontend = v;
        }
        if let Ok(v) = std::env::var("EISENBAHN_BROKER_BACKEND") {
            self.broker.backend = v;
        }
        if let Ok(v) = std::env::var("EISENBAHN_BROKER_METRICS_PORT") {
            if let Ok(port) = v.parse::<u16>() {
                self.broker.metrics_port = Some(port);
            }
        }
        if let Ok(v) = std::env::var("EISENBAHN_TRANSPORT_KIND") {
            self.transport.kind = v;
        }
        if let Ok(v) = std::env::var("EISENBAHN_TRANSPORT_DEFAULT_HOST") {
            self.transport.default_host = v;
        }
        if let Ok(v) = std::env::var("EISENBAHN_TRANSPORT_BASE_PORT") {
            if let Ok(port) = v.parse::<u16>() {
                self.transport.base_port = port;
            }
        }
    }

    // ── Validation ──────────────────────────────────────────────────

    /// Validate the config: check for circular dependencies, missing references, etc.
    pub fn validate(&self) -> Result<(), EisenbahnError> {
        self.validate_pipeline_references()?;
        self.validate_no_circular_dependencies()?;
        self.validate_worker_pipelines()?;
        self.validate_transport_kind()?;
        Ok(())
    }

    /// Ensure all `after` references in pipeline stages point to existing stages.
    fn validate_pipeline_references(&self) -> Result<(), EisenbahnError> {
        for (name, stage) in &self.pipeline.stages {
            for dep in &stage.after {
                if !self.pipeline.stages.contains_key(dep) {
                    return Err(EisenbahnError::Config(format!(
                        "pipeline stage '{name}' references unknown upstream stage '{dep}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Detect circular dependencies in the pipeline DAG.
    fn validate_no_circular_dependencies(&self) -> Result<(), EisenbahnError> {
        topological_sort(&self.pipeline.stages)?;
        Ok(())
    }

    /// Ensure worker pipeline references point to existing stages.
    fn validate_worker_pipelines(&self) -> Result<(), EisenbahnError> {
        for (name, worker) in &self.workers {
            for pipeline in &worker.pipelines {
                if !self.pipeline.stages.is_empty()
                    && !self.pipeline.stages.contains_key(pipeline)
                {
                    return Err(EisenbahnError::Config(format!(
                        "worker '{name}' references unknown pipeline stage '{pipeline}'"
                    )));
                }
            }
        }
        Ok(())
    }

    /// Ensure transport kind is valid.
    fn validate_transport_kind(&self) -> Result<(), EisenbahnError> {
        match self.transport.kind.as_str() {
            "ipc" | "tcp" => Ok(()),
            other => Err(EisenbahnError::Config(format!(
                "invalid transport kind '{other}', expected 'ipc' or 'tcp'"
            ))),
        }
    }
}

impl Default for EisenbahnConfig {
    fn default() -> Self {
        Self::local()
    }
}

// ── Helpers ─────────────────────────────────────────────────────────

/// Parse an endpoint string like "ipc:///tmp/foo.sock" or "tcp://host:port" into a Transport.
fn parse_endpoint_to_transport(endpoint: &str) -> Transport {
    if let Some(path) = endpoint.strip_prefix("ipc://") {
        let name = Path::new(path)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("unknown");
        Transport::ipc(name)
    } else if let Some(addr) = endpoint.strip_prefix("tcp://") {
        if let Some((host, port_str)) = addr.rsplit_once(':') {
            let port = port_str.parse().unwrap_or(5555);
            Transport::tcp(host, port)
        } else {
            Transport::tcp(addr, 5555)
        }
    } else {
        Transport::ipc("unknown")
    }
}

/// Topological sort using Kahn's algorithm.
///
/// Returns stage names in dependency order, or an error if a cycle is detected.
fn topological_sort(
    stages: &HashMap<String, StageConfig>,
) -> Result<Vec<String>, EisenbahnError> {
    if stages.is_empty() {
        return Ok(Vec::new());
    }

    // Build adjacency list and in-degree map
    let mut in_degree: HashMap<&str, usize> = HashMap::new();
    let mut dependents: HashMap<&str, Vec<&str>> = HashMap::new();

    for name in stages.keys() {
        in_degree.entry(name.as_str()).or_insert(0);
        dependents.entry(name.as_str()).or_default();
    }

    for (name, stage) in stages {
        for dep in &stage.after {
            dependents
                .entry(dep.as_str())
                .or_default()
                .push(name.as_str());
            *in_degree.entry(name.as_str()).or_insert(0) += 1;
        }
    }

    let mut queue: VecDeque<&str> = in_degree
        .iter()
        .filter(|(_, &deg)| deg == 0)
        .map(|(&name, _)| name)
        .collect();

    let mut sorted = Vec::with_capacity(stages.len());

    while let Some(node) = queue.pop_front() {
        sorted.push(node.to_string());
        if let Some(deps) = dependents.get(node) {
            for &dep in deps {
                if let Some(deg) = in_degree.get_mut(dep) {
                    *deg -= 1;
                    if *deg == 0 {
                        queue.push_back(dep);
                    }
                }
            }
        }
    }

    if sorted.len() != stages.len() {
        let in_cycle: Vec<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg > 0)
            .map(|(&name, _)| name)
            .collect();
        return Err(EisenbahnError::CircularDependency(format!(
            "cycle detected among stages: {}",
            in_cycle.join(" → ")
        )));
    }

    Ok(sorted)
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_toml() {
        let toml = r#"
[broker]
frontend = "ipc:///tmp/stupid-db/broker-fe.sock"
backend = "ipc:///tmp/stupid-db/broker-be.sock"
"#;
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        assert!(cfg.broker.frontend.contains("broker-fe"));
        assert!(cfg.broker.backend.contains("broker-be"));
        assert!(cfg.workers.is_empty());
    }

    #[test]
    fn parse_full_toml() {
        let toml = r#"
[broker]
frontend = "tcp://10.0.0.1:5555"
backend = "tcp://10.0.0.1:5556"
metrics_port = 9090

[transport]
kind = "tcp"
default_host = "10.0.0.1"
base_port = 5560

[workers.ingest]
binary = "stupid-ingest"
subscriptions = ["raw.parquet"]
pipelines = ["ingest"]
instances = 2

[workers.compute]
binary = "stupid-compute"
subscriptions = ["entity.created"]
pipelines = ["compute"]

[pipeline.stages.ingest]
concurrency = 4

[pipeline.stages.compute]
after = ["ingest"]
concurrency = 2

[pipeline.stages.graph]
after = ["compute"]
endpoint = "tcp://10.0.0.1:5570"
"#;
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.broker.metrics_port, Some(9090));
        assert_eq!(cfg.workers.len(), 2);
        assert_eq!(cfg.workers["ingest"].instances, 2);
        assert_eq!(cfg.workers["compute"].instances, 1); // default
        assert_eq!(cfg.pipeline.stages.len(), 3);

        let order = cfg.pipeline_order().unwrap();
        let ingest_pos = order.iter().position(|s| s == "ingest").unwrap();
        let compute_pos = order.iter().position(|s| s == "compute").unwrap();
        let graph_pos = order.iter().position(|s| s == "graph").unwrap();
        assert!(ingest_pos < compute_pos);
        assert!(compute_pos < graph_pos);
    }

    #[test]
    fn detect_circular_dependency() {
        let toml = r#"
[broker]

[pipeline.stages.a]
after = ["c"]

[pipeline.stages.b]
after = ["a"]

[pipeline.stages.c]
after = ["b"]
"#;
        let err = EisenbahnConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("circular") || msg.contains("cycle"),
            "got: {msg}"
        );
    }

    #[test]
    fn detect_missing_upstream_reference() {
        let toml = r#"
[broker]

[pipeline.stages.compute]
after = ["nonexistent"]
"#;
        let err = EisenbahnConfig::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("nonexistent"));
    }

    #[test]
    fn detect_worker_bad_pipeline_ref() {
        let toml = r#"
[broker]

[pipeline.stages.ingest]

[workers.bad]
binary = "bad-worker"
pipelines = ["missing_stage"]
"#;
        let err = EisenbahnConfig::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("missing_stage"));
    }

    #[test]
    fn detect_invalid_transport_kind() {
        let toml = r#"
[broker]

[transport]
kind = "udp"
"#;
        let err = EisenbahnConfig::from_toml(toml).unwrap_err();
        assert!(err.to_string().contains("udp"));
    }

    #[test]
    fn env_override_broker_frontend() {
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe {
            std::env::set_var("EISENBAHN_BROKER_FRONTEND", "tcp://override:9999");
        }
        let toml = r#"
[broker]
frontend = "ipc:///tmp/original.sock"
backend = "ipc:///tmp/backend.sock"
"#;
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.broker.frontend, "tcp://override:9999");
        unsafe {
            std::env::remove_var("EISENBAHN_BROKER_FRONTEND");
        }
    }

    #[test]
    fn env_override_transport_base_port() {
        // SAFETY: test-only, nextest runs each test in its own process
        unsafe {
            std::env::set_var("EISENBAHN_TRANSPORT_BASE_PORT", "7777");
        }
        let toml = "[broker]\n";
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        assert_eq!(cfg.transport.base_port, 7777);
        unsafe {
            std::env::remove_var("EISENBAHN_TRANSPORT_BASE_PORT");
        }
    }

    #[test]
    fn local_config_defaults() {
        let cfg = EisenbahnConfig::local();
        assert!(cfg.broker.frontend.contains("ipc://"));
        assert_eq!(cfg.transport.kind, "ipc");
        assert!(cfg.workers.is_empty());
    }

    #[test]
    fn distributed_config() {
        let cfg = EisenbahnConfig::distributed("10.0.0.1", 5555);
        assert_eq!(cfg.broker.frontend, "tcp://10.0.0.1:5555");
        assert_eq!(cfg.broker.backend, "tcp://10.0.0.1:5556");
        assert_eq!(cfg.broker.metrics_port, Some(5557));
        assert_eq!(cfg.transport.kind, "tcp");
    }

    #[test]
    fn broker_transport_resolution() {
        let cfg = EisenbahnConfig::distributed("10.0.0.1", 5555);
        let frontend = cfg.broker_frontend_transport();
        assert_eq!(frontend.endpoint(), "tcp://10.0.0.1:5555");
    }

    #[test]
    fn empty_pipeline_is_valid() {
        let toml = "[broker]\n";
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        assert!(cfg.pipeline_order().unwrap().is_empty());
    }

    #[test]
    fn worker_env_vars() {
        let toml = r#"
[broker]

[workers.compute]
binary = "stupid-compute"

[workers.compute.env]
RUST_LOG = "debug"
CUDA_VISIBLE_DEVICES = "0,1"
"#;
        let cfg = EisenbahnConfig::from_toml(toml).unwrap();
        let w = &cfg.workers["compute"];
        assert_eq!(w.env["RUST_LOG"], "debug");
        assert_eq!(w.env["CUDA_VISIBLE_DEVICES"], "0,1");
    }

    #[test]
    fn parse_endpoint_ipc() {
        let t = parse_endpoint_to_transport("ipc:///tmp/stupid-db/broker-frontend.sock");
        assert_eq!(t.endpoint(), "ipc:///tmp/stupid-db/broker-frontend.sock");
    }

    #[test]
    fn parse_endpoint_tcp() {
        let t = parse_endpoint_to_transport("tcp://10.0.0.1:5555");
        assert_eq!(t.endpoint(), "tcp://10.0.0.1:5555");
    }

    #[test]
    fn topological_sort_linear() {
        let mut stages = HashMap::new();
        stages.insert(
            "a".into(),
            StageConfig { after: vec![], endpoint: None, concurrency: 1 },
        );
        stages.insert(
            "b".into(),
            StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
        );
        stages.insert(
            "c".into(),
            StageConfig { after: vec!["b".into()], endpoint: None, concurrency: 1 },
        );

        let order = topological_sort(&stages).unwrap();
        let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn topological_sort_diamond() {
        let mut stages = HashMap::new();
        stages.insert(
            "a".into(),
            StageConfig { after: vec![], endpoint: None, concurrency: 1 },
        );
        stages.insert(
            "b".into(),
            StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
        );
        stages.insert(
            "c".into(),
            StageConfig { after: vec!["a".into()], endpoint: None, concurrency: 1 },
        );
        stages.insert(
            "d".into(),
            StageConfig {
                after: vec!["b".into(), "c".into()],
                endpoint: None,
                concurrency: 1,
            },
        );

        let order = topological_sort(&stages).unwrap();
        let pos = |s: &str| order.iter().position(|x| x == s).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }

    #[test]
    fn self_referencing_stage_is_circular() {
        let toml = r#"
[broker]

[pipeline.stages.loop]
after = ["loop"]
"#;
        let err = EisenbahnConfig::from_toml(toml).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("circular") || msg.contains("cycle"),
            "got: {msg}"
        );
    }
}
