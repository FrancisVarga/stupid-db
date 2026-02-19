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
    EisenbahnConfig, EisenbahnError, EventPublisher, EventSubscriber, Message, RequestSender,
    Worker, WorkerBuilder, WorkerRunner, ZmqPublisher, ZmqRequestClient, ZmqSubscriber,
};
use stupid_eisenbahn::services::{
    AgentServiceRequest, AgentServiceResponse, AthenaServiceRequest,
    CatalogQueryRequest, CatalogQueryResponse, QueryServiceRequest, QueryServiceResponse,
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
    // Service request clients (DEALER sockets for request/reply routing).
    query_client: Option<Arc<ZmqRequestClient>>,
    agent_client: Option<Arc<ZmqRequestClient>>,
    athena_client: Option<Arc<ZmqRequestClient>>,
    catalog_client: Option<Arc<ZmqRequestClient>>,
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

        // Connect service DEALER clients for any configured service endpoints.
        let query_client = connect_service_client(&eisenbahn_config, "query").await;
        let agent_client = connect_service_client(&eisenbahn_config, "agent").await;
        let athena_client = connect_service_client(&eisenbahn_config, "athena").await;
        let catalog_client = connect_service_client(&eisenbahn_config, "catalog").await;

        let client = Arc::new(Self {
            publisher,
            subscriber,
            shutdown: Arc::new(Notify::new()),
            ws_broadcast,
            query_client,
            agent_client,
            athena_client,
            catalog_client,
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

    /// Publish an event message to a topic (best-effort, non-panicking).
    ///
    /// Returns `Ok(())` on success, `Err` if serialization or publishing fails.
    /// Callers should treat failures as non-fatal (logging a warning).
    pub async fn publish_event<T: serde::Serialize>(
        &self,
        topic: &str,
        event: &T,
    ) -> Result<(), EisenbahnError> {
        let msg = Message::new(topic, event)?;
        self.publisher.publish(msg).await
    }

    /// Trigger graceful shutdown of the eisenbahn client.
    #[allow(dead_code)] // Will be used when server graceful shutdown is implemented
    pub fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }

    // ── Service request helpers ─────────────────────────────────────

    /// Send a query request to the query-worker and get the response.
    pub async fn query(
        &self,
        request: QueryServiceRequest,
        timeout: Duration,
    ) -> Result<QueryServiceResponse, EisenbahnError> {
        let client = self.query_client.as_ref()
            .ok_or_else(|| EisenbahnError::Config("query service not configured".into()))?;
        let msg = Message::new(topics::SVC_QUERY_REQUEST, &request)?;
        let reply = client.request(msg, timeout).await?;
        Ok(reply.decode::<QueryServiceResponse>()?)
    }

    /// Send an agent request to the agent-worker and get the response.
    pub async fn agent_execute(
        &self,
        request: AgentServiceRequest,
        timeout: Duration,
    ) -> Result<AgentServiceResponse, EisenbahnError> {
        let client = self.agent_client.as_ref()
            .ok_or_else(|| EisenbahnError::Config("agent service not configured".into()))?;
        let msg = Message::new(topics::SVC_AGENT_REQUEST, &request)?;
        let reply = client.request(msg, timeout).await?;
        Ok(reply.decode::<AgentServiceResponse>()?)
    }

    /// Send an athena request and get a stream of response chunks.
    pub async fn athena_query_stream(
        &self,
        request: AthenaServiceRequest,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Message, EisenbahnError>>, EisenbahnError> {
        let client = self.athena_client.as_ref()
            .ok_or_else(|| EisenbahnError::Config("athena service not configured".into()))?;
        let msg = Message::new(topics::SVC_ATHENA_REQUEST, &request)?;
        client.request_stream(msg).await
    }

    /// Send a catalog query request.
    pub async fn catalog_query(
        &self,
        request: CatalogQueryRequest,
        timeout: Duration,
    ) -> Result<CatalogQueryResponse, EisenbahnError> {
        let client = self.catalog_client.as_ref()
            .ok_or_else(|| EisenbahnError::Config("catalog service not configured".into()))?;
        let msg = Message::new(topics::SVC_CATALOG_REQUEST, &request)?;
        let reply = client.request(msg, timeout).await?;
        Ok(reply.decode::<CatalogQueryResponse>()?)
    }

    /// Check if a specific service is available.
    pub fn has_service(&self, name: &str) -> bool {
        match name {
            "query" => self.query_client.is_some(),
            "agent" => self.agent_client.is_some(),
            "athena" => self.athena_client.is_some(),
            "catalog" => self.catalog_client.is_some(),
            _ => false,
        }
    }
}

/// Connect a DEALER client to a named service, logging success or skip.
async fn connect_service_client(
    config: &EisenbahnConfig,
    name: &str,
) -> Option<Arc<ZmqRequestClient>> {
    let transport = config.service_transport(name)?;
    match ZmqRequestClient::connect(&transport).await {
        Ok(client) => {
            info!(service = %name, endpoint = %transport.endpoint(), "connected service client");
            Some(Arc::new(client))
        }
        Err(e) => {
            warn!(service = %name, error = %e, "failed to connect service client — skipping");
            None
        }
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
