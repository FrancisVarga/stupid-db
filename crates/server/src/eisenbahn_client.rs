//! Eisenbahn integration — opt-in ZMQ-based messaging for the API gateway.
//!
//! When the `--eisenbahn` flag is passed, the server registers as a Worker
//! in the eisenbahn messaging network: publishing health pings, subscribing
//! to events from other workers, and optionally routing requests through ZMQ
//! instead of direct crate calls.
//!
//! When the flag is NOT passed, this module is never initialized and the
//! server operates exactly as before (direct function calls).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;
use tracing::{info, warn};

use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventSubscriber, Message, Worker,
    WorkerBuilder, WorkerRunner, ZmqPublisher, ZmqSubscriber,
};
use stupid_eisenbahn::topics;

/// Configuration for the eisenbahn client embedded in the server.
pub struct EisenbahnClientConfig {
    /// Path to the eisenbahn.toml config file.
    pub config_path: String,
}

impl Default for EisenbahnClientConfig {
    fn default() -> Self {
        Self {
            config_path: "config/eisenbahn.toml".to_string(),
        }
    }
}

/// The server's eisenbahn client — makes the server a participant in the
/// ZMQ messaging network alongside the dedicated worker processes.
///
/// Responsibilities:
/// - Publish periodic health pings so the broker knows the API gateway is alive
/// - Subscribe to worker events (ingest.complete, compute.complete, anomaly.detected)
///   for real-time dashboard push via WebSocket broadcast
/// - (Future) Route compute/graph/storage requests through ZMQ instead of direct calls
pub struct EisenbahnClient {
    publisher: Arc<ZmqPublisher>,
    subscriber: Arc<ZmqSubscriber>,
    shutdown: Arc<Notify>,
    /// Broadcast sender for forwarding eisenbahn events to WebSocket clients.
    ws_broadcast: tokio::sync::broadcast::Sender<String>,
}

impl EisenbahnClient {
    /// Connect to the eisenbahn broker and create the client.
    pub async fn connect(
        config: &EisenbahnClientConfig,
        ws_broadcast: tokio::sync::broadcast::Sender<String>,
    ) -> Result<Arc<Self>, EisenbahnError> {
        let eisenbahn_config = match EisenbahnConfig::from_file(&config.config_path) {
            Ok(cfg) => {
                info!(path = %config.config_path, "loaded eisenbahn config");
                cfg
            }
            Err(e) => {
                warn!(
                    error = %e,
                    path = %config.config_path,
                    "failed to load eisenbahn config, using local defaults"
                );
                EisenbahnConfig::local()
            }
        };

        let publisher = Arc::new(
            ZmqPublisher::connect(&eisenbahn_config.broker_frontend_transport()).await?,
        );
        let subscriber = Arc::new(
            ZmqSubscriber::connect(&eisenbahn_config.broker_backend_transport()).await?,
        );

        let client = Arc::new(Self {
            publisher,
            subscriber,
            shutdown: Arc::new(Notify::new()),
            ws_broadcast,
        });

        Ok(client)
    }

    /// Start the eisenbahn event loop and worker runner in background tasks.
    ///
    /// This spawns two tasks:
    /// 1. An event subscription loop that forwards events to the WebSocket broadcast
    /// 2. The WorkerRunner for health pings and lifecycle management
    pub async fn start(self: &Arc<Self>) {
        // Subscribe to all eisenbahn events
        let topics_to_subscribe = [
            topics::INGEST_COMPLETE,
            topics::COMPUTE_COMPLETE,
            topics::ANOMALY_DETECTED,
            topics::WORKER_HEALTH,
        ];

        for topic in &topics_to_subscribe {
            if let Err(e) = self.subscriber.subscribe(topic).await {
                warn!(topic = %topic, error = %e, "failed to subscribe to eisenbahn topic");
            }
        }

        // Spawn event forwarding loop
        let client = self.clone();
        tokio::spawn(async move {
            client.event_loop().await;
        });

        // Spawn the WorkerRunner for health pings
        let worker: Arc<dyn Worker> = self.clone();
        let publisher = self.publisher.clone();
        let shutdown = self.shutdown.clone();

        let runner_config = WorkerBuilder::new("api-gateway")
            .health_interval(Duration::from_secs(30))
            .shutdown_timeout(Duration::from_secs(5))
            .subscribe(topics::INGEST_COMPLETE)
            .subscribe(topics::COMPUTE_COMPLETE)
            .subscribe(topics::ANOMALY_DETECTED)
            .build();

        tokio::spawn(async move {
            if let Err(e) = WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await {
                warn!(error = %e, "eisenbahn worker runner exited with error");
            }
        });

        info!("eisenbahn client started — server registered as api-gateway worker");
    }

    /// Forward eisenbahn events to the WebSocket broadcast channel.
    async fn event_loop(self: &Arc<Self>) {
        loop {
            tokio::select! {
                result = EventSubscriber::recv(self.subscriber.as_ref()) => {
                    match result {
                        Ok(msg) => {
                            self.handle_event(msg);
                        }
                        Err(e) => {
                            warn!(error = %e, "eisenbahn subscriber recv error");
                            tokio::time::sleep(Duration::from_millis(500)).await;
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    info!("eisenbahn event loop shutting down");
                    break;
                }
            }
        }
    }

    /// Convert an eisenbahn message into a JSON string and broadcast to WebSocket clients.
    fn handle_event(&self, msg: Message) {
        // Build a simple JSON envelope for WebSocket consumers
        let json = serde_json::json!({
            "source": "eisenbahn",
            "topic": msg.topic,
            "timestamp": msg.timestamp.to_rfc3339(),
            "correlation_id": msg.correlation_id.to_string(),
        });

        if let Ok(text) = serde_json::to_string(&json) {
            // Best-effort broadcast — if no WebSocket clients are listening, that's fine
            let _ = self.ws_broadcast.send(text);
        }
    }

    /// Trigger graceful shutdown of the eisenbahn client.
    #[allow(dead_code)] // Will be used when server graceful shutdown is implemented
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

#[async_trait]
impl Worker for EisenbahnClient {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("api-gateway worker started");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("api-gateway worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "api-gateway"
    }
}
