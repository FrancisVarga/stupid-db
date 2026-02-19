use std::collections::HashMap;
use std::path::Path;

use crate::error::EisenbahnError;
use crate::transport::Transport;

use super::helpers::parse_endpoint_to_transport;
use super::helpers::topological_sort;
use super::types::{BrokerConfig, EisenbahnConfig, PipelineTopology, TransportConfig};

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
    /// - `EISENBAHN_BROKER_FRONTEND` -> `broker.frontend`
    /// - `EISENBAHN_BROKER_BACKEND` -> `broker.backend`
    /// - `EISENBAHN_BROKER_METRICS_PORT` -> `broker.metrics_port`
    /// - `EISENBAHN_TRANSPORT_KIND` -> `transport.kind`
    /// - `EISENBAHN_TRANSPORT_DEFAULT_HOST` -> `transport.default_host`
    /// - `EISENBAHN_TRANSPORT_BASE_PORT` -> `transport.base_port`
    pub(crate) fn apply_env_overrides(&mut self) {
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
}

impl Default for EisenbahnConfig {
    fn default() -> Self {
        Self::local()
    }
}
