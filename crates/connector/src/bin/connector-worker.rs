//! connector-worker — Eisenbahn worker wrapping entity extraction.
//!
//! Subscribes to events:
//! - `eisenbahn.compute.complete` — triggers entity/edge extraction from features
//!
//! The connector bridges compute results to graph updates by extracting
//! entities and deriving edges from processed data.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::info;

use stupid_eisenbahn::events::ComputeComplete;
use stupid_eisenbahn::topics;
use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventSubscriber, Message, Worker,
    WorkerBuilder, WorkerRunner, ZmqPublisher, ZmqSubscriber,
};

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn connector worker — entity extraction and edge derivation.
#[derive(Parser, Debug)]
#[command(name = "connector-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "CONNECTOR_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "CONNECTOR_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── ConnectorWorker ─────────────────────────────────────────────────

/// Wraps the existing connector library as an eisenbahn worker.
struct ConnectorWorker {
    publisher: Arc<ZmqPublisher>,
    subscriber: Arc<ZmqSubscriber>,
    shutdown: Arc<Notify>,
}

impl ConnectorWorker {
    /// Handle an incoming event message.
    async fn handle_event(&self, msg: Message) -> Result<(), EisenbahnError> {
        match msg.topic.as_str() {
            topics::COMPUTE_COMPLETE => {
                let event: ComputeComplete =
                    msg.decode().map_err(EisenbahnError::Deserialization)?;
                info!(
                    batch_id = %event.batch_id,
                    features = event.features_computed,
                    "compute complete — entity extraction pending"
                );
                // TODO: run entity_extract on computed features
            }
            other => {
                tracing::warn!(topic = %other, "unexpected event topic");
            }
        }
        Ok(())
    }

    /// Run the event loop: receive events from the broker.
    async fn run_loop(self: &Arc<Self>) {
        loop {
            tokio::select! {
                result = EventSubscriber::recv(self.subscriber.as_ref()) => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_event(msg).await {
                                tracing::error!(error = %e, "failed to handle event");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "subscriber recv error");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    info!("connector worker event loop shutting down");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl Worker for ConnectorWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        self.subscriber
            .subscribe(topics::COMPUTE_COMPLETE)
            .await?;
        info!("connector worker started — subscribed to compute.complete");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("connector worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "connector-worker"
    }
}

// ── main ────────────────────────────────────────────────────────────

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();

    let config = match EisenbahnConfig::from_file(&cli.config) {
        Ok(cfg) => {
            info!(path = %cli.config, "loaded eisenbahn config");
            cfg
        }
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %cli.config,
                "failed to load config, using local defaults"
            );
            EisenbahnConfig::local()
        }
    };

    let publisher: Arc<ZmqPublisher> = Arc::new(
        ZmqPublisher::connect(&config.broker_frontend_transport()).await?,
    );
    let subscriber = Arc::new(
        ZmqSubscriber::connect(&config.broker_backend_transport()).await?,
    );

    let shutdown = Arc::new(Notify::new());

    let worker = Arc::new(ConnectorWorker {
        publisher: publisher.clone(),
        subscriber,
        shutdown: shutdown.clone(),
    });

    // Spawn the event loop
    let worker_for_loop = worker.clone();
    tokio::spawn(async move {
        worker_for_loop.run_loop().await;
    });

    let runner_config = WorkerBuilder::new("connector-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::COMPUTE_COMPLETE)
        .build();

    info!("connector-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("connector-worker exited cleanly");
    Ok(())
}
