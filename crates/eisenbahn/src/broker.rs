use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use tokio::sync::Mutex;
use zeromq::prelude::*;
use zeromq::{PubSocket, RepSocket, SubSocket, ZmqMessage};

use crate::metrics::MetricsCollector;
use crate::messages::events::WorkerHealth;
use crate::messages::topics::WORKER_HEALTH;
use crate::transport::Transport;

/// Metrics collected by the broker during message proxying.
#[derive(Debug)]
pub struct BrokerMetrics {
    /// Total messages forwarded through the proxy.
    pub total_messages: AtomicU64,
    /// Per-topic message counts.
    pub topic_counts: Mutex<HashMap<String, u64>>,
}

impl BrokerMetrics {
    fn new() -> Self {
        Self {
            total_messages: AtomicU64::new(0),
            topic_counts: Mutex::new(HashMap::new()),
        }
    }

    /// Snapshot of total forwarded messages.
    pub fn total(&self) -> u64 {
        self.total_messages.load(Ordering::Relaxed)
    }
}

/// Configuration for the event broker.
#[derive(Debug, Clone)]
pub struct BrokerConfig {
    /// Frontend endpoint where publishers connect (broker binds SUB here).
    pub frontend: Transport,
    /// Backend endpoint where subscribers connect (broker binds PUB here).
    pub backend: Transport,
    /// Health check endpoint (REP socket for liveness probes).
    pub health: Transport,
    /// Optional HTTP port for the `/metrics` JSON endpoint.
    pub metrics_port: Option<u16>,
}

impl BrokerConfig {
    /// Create a local IPC broker configuration.
    pub fn local() -> Self {
        Self {
            frontend: Transport::ipc("broker-frontend"),
            backend: Transport::ipc("broker-backend"),
            health: Transport::ipc("broker-health"),
            metrics_port: None,
        }
    }

    /// Create a TCP broker configuration.
    pub fn tcp(host: &str, frontend_port: u16, backend_port: u16, health_port: u16) -> Self {
        Self {
            frontend: Transport::tcp(host, frontend_port),
            backend: Transport::tcp(host, backend_port),
            health: Transport::tcp(host, health_port),
            metrics_port: None,
        }
    }
}

impl Default for BrokerConfig {
    fn default() -> Self {
        Self::local()
    }
}

/// XPUB/XSUB-style event broker using PUB/SUB as the underlying transport.
///
/// The broker acts as a central rendezvous point:
/// - Publishers connect to the **frontend** (SUB socket that the broker binds).
/// - Subscribers connect to the **backend** (PUB socket that the broker binds).
/// - Messages received on frontend are forwarded to backend with topic logging.
///
/// Since `zeromq` 0.4 does not provide XPUB/XSUB socket types, we emulate
/// the proxy pattern with PUB+SUB. The broker subscribes to all topics ("").
pub struct EventBroker {
    config: BrokerConfig,
    metrics: Arc<BrokerMetrics>,
    collector: MetricsCollector,
    shutdown: Arc<AtomicBool>,
}

impl EventBroker {
    /// Create a new broker with the given configuration.
    pub fn new(config: BrokerConfig) -> Self {
        Self {
            config,
            metrics: Arc::new(BrokerMetrics::new()),
            collector: MetricsCollector::new(),
            shutdown: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Access the broker's basic metrics (total counts).
    pub fn metrics(&self) -> &Arc<BrokerMetrics> {
        &self.metrics
    }

    /// Access the rich metrics collector (throughput, worker health, time-series).
    pub fn collector(&self) -> &MetricsCollector {
        &self.collector
    }

    /// Signal the broker to shut down gracefully.
    pub fn shutdown(&self) {
        self.shutdown.store(true, Ordering::SeqCst);
    }

    /// Run the broker proxy loop.
    ///
    /// This binds three sockets:
    /// 1. SUB (frontend) — receives from publishers, subscribed to all topics.
    /// 2. PUB (backend) — forwards to subscribers.
    /// 3. REP (health) — responds to health check pings with "ok".
    ///
    /// Returns when shutdown is signaled or an unrecoverable error occurs.
    pub async fn run(&self) -> Result<(), crate::error::EisenbahnError> {
        // -- Frontend: SUB socket that publishers connect to --
        let mut frontend = SubSocket::new();
        frontend
            .bind(&self.config.frontend.endpoint())
            .await
            .map_err(crate::error::EisenbahnError::Zmq)?;
        // Subscribe to all topics so every message is forwarded.
        frontend
            .subscribe("")
            .await
            .map_err(crate::error::EisenbahnError::Zmq)?;

        tracing::info!(
            endpoint = %self.config.frontend.endpoint(),
            "broker frontend (SUB) bound — publishers connect here"
        );

        // -- Backend: PUB socket that subscribers connect to --
        let mut backend = PubSocket::new();
        backend
            .bind(&self.config.backend.endpoint())
            .await
            .map_err(crate::error::EisenbahnError::Zmq)?;

        tracing::info!(
            endpoint = %self.config.backend.endpoint(),
            "broker backend (PUB) bound — subscribers connect here"
        );

        // -- Health check: REP socket --
        let mut health = RepSocket::new();
        health
            .bind(&self.config.health.endpoint())
            .await
            .map_err(crate::error::EisenbahnError::Zmq)?;

        tracing::info!(
            endpoint = %self.config.health.endpoint(),
            "broker health check (REP) bound"
        );

        // Spawn health check responder in background.
        let shutdown_flag = self.shutdown.clone();
        tokio::spawn(async move {
            Self::health_loop(&mut health, &shutdown_flag).await;
        });

        // -- Metrics subsystem --
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let collector = self.collector.clone();

        // Spawn the 1-second tick task for rate windowing.
        let _tick_handle =
            crate::metrics::spawn_tick_task(collector.clone(), shutdown_rx.clone());

        // Optionally spawn the HTTP metrics server.
        let _http_handle = if let Some(port) = self.config.metrics_port {
            Some(crate::metrics::spawn_metrics_server(
                port,
                collector.clone(),
                shutdown_rx,
            ))
        } else {
            None
        };

        // -- Main proxy loop --
        let metrics = self.metrics.clone();
        let shutdown = self.shutdown.clone();

        tracing::info!("broker proxy loop started");

        loop {
            if shutdown.load(Ordering::SeqCst) {
                tracing::info!("broker shutting down");
                break;
            }

            // Use a timeout so we periodically check the shutdown flag.
            let recv_result =
                tokio::time::timeout(std::time::Duration::from_millis(100), frontend.recv()).await;

            let msg = match recv_result {
                Ok(Ok(msg)) => msg,
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "frontend recv error");
                    continue;
                }
                Err(_) => {
                    // Timeout — loop back to check shutdown flag.
                    continue;
                }
            };

            // Extract topic from first frame for metrics.
            let topic = extract_topic(&msg);

            // Compute byte size across all frames.
            let byte_size: u64 = msg.iter().map(|f| f.len() as u64).sum();

            // Update basic metrics (backward compat).
            metrics.total_messages.fetch_add(1, Ordering::Relaxed);
            {
                let mut counts = metrics.topic_counts.lock().await;
                *counts.entry(topic.clone()).or_insert(0) += 1;
            }

            // Feed the rich metrics collector.
            collector.record_message(&topic, byte_size).await;

            // If this is a worker health message, try to decode and track it.
            if topic == WORKER_HEALTH {
                if let Some(health) = try_decode_worker_health(&msg) {
                    collector.record_worker_health(&health).await;
                }
            }

            tracing::debug!(
                topic = %topic,
                bytes = byte_size,
                total = metrics.total_messages.load(Ordering::Relaxed),
                "forwarding message"
            );

            // Forward to backend (PUB).
            if let Err(e) = backend.send(msg).await {
                tracing::warn!(error = %e, "backend send error");
            }
        }

        // Signal metrics subsystem to shut down.
        let _ = shutdown_tx.send(true);

        tracing::info!(
            total = metrics.total_messages.load(Ordering::Relaxed),
            "broker stopped"
        );

        Ok(())
    }

    /// Health check responder loop — replies "ok" to any REQ.
    async fn health_loop(health: &mut RepSocket, shutdown: &AtomicBool) {
        loop {
            if shutdown.load(Ordering::SeqCst) {
                break;
            }

            let recv_result =
                tokio::time::timeout(std::time::Duration::from_millis(500), health.recv()).await;

            match recv_result {
                Ok(Ok(_request)) => {
                    let reply: ZmqMessage = "ok".into();
                    if let Err(e) = health.send(reply).await {
                        tracing::warn!(error = %e, "health reply error");
                    }
                }
                Ok(Err(e)) => {
                    tracing::warn!(error = %e, "health recv error");
                }
                Err(_) => {
                    // Timeout, loop back.
                }
            }
        }
    }
}

/// Extract a topic string from the first frame of a ZMQ message.
///
/// In ZeroMQ PUB/SUB, the first frame typically contains the topic prefix.
/// We attempt to interpret it as UTF-8; if that fails, we use a hex representation.
fn extract_topic(msg: &ZmqMessage) -> String {
    msg.iter()
        .next()
        .map(|frame| {
            String::from_utf8(frame.to_vec()).unwrap_or_else(|_| hex::encode(frame.as_ref()))
        })
        .unwrap_or_else(|| "<empty>".to_string())
}

/// Try to decode a `WorkerHealth` payload from a ZMQ message.
///
/// Worker health messages use the `Message` envelope format — the second frame
/// (or the full first frame beyond the topic prefix) contains the MessagePack-encoded
/// envelope. We attempt to deserialize it; on failure we silently return `None`.
fn try_decode_worker_health(msg: &ZmqMessage) -> Option<WorkerHealth> {
    // ZMQ PUB/SUB: first frame is topic, remaining frames are the payload.
    let frames: Vec<_> = msg.iter().collect();
    if frames.len() < 2 {
        return None;
    }
    let envelope = crate::Message::from_bytes(frames[1].as_ref()).ok()?;
    envelope.decode::<WorkerHealth>().ok()
}

/// Minimal hex encoding (avoids pulling in the `hex` crate).
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn broker_config_local_endpoints() {
        let cfg = BrokerConfig::local();
        assert!(cfg.frontend.endpoint().contains("broker-frontend"));
        assert!(cfg.backend.endpoint().contains("broker-backend"));
        assert!(cfg.health.endpoint().contains("broker-health"));
    }

    #[test]
    fn broker_config_tcp_endpoints() {
        let cfg = BrokerConfig::tcp("0.0.0.0", 5555, 5556, 5557);
        assert_eq!(cfg.frontend.endpoint(), "tcp://0.0.0.0:5555");
        assert_eq!(cfg.backend.endpoint(), "tcp://0.0.0.0:5556");
        assert_eq!(cfg.health.endpoint(), "tcp://0.0.0.0:5557");
    }

    #[test]
    fn metrics_default_zero() {
        let m = BrokerMetrics::new();
        assert_eq!(m.total(), 0);
    }

    #[test]
    fn extract_topic_from_utf8_frame() {
        let msg: ZmqMessage = "entity.created".into();
        assert_eq!(extract_topic(&msg), "entity.created");
    }

    #[test]
    fn hex_encode_works() {
        assert_eq!(hex::encode(&[0xde, 0xad]), "dead");
    }
}
