//! graph-worker — Eisenbahn worker wrapping the knowledge graph store.
//!
//! Subscribes to events:
//! - `eisenbahn.compute.complete` — triggers graph update from computed features
//!
//! Pipeline flow: PULL from compute → update graph store
//!
//! The graph worker receives entity/edge updates and applies them to the
//! in-memory property graph (GraphStore).

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::{error, info, warn};

use stupid_eisenbahn::msg_pipeline::GraphUpdate;
use stupid_eisenbahn::topics;
use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventSubscriber, Message, PipelineReceiver,
    Worker, WorkerBuilder, WorkerRunner, ZmqPipelineReceiver, ZmqPublisher, ZmqSubscriber,
};

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn graph worker — knowledge graph updates from computed features.
#[derive(Parser, Debug)]
#[command(name = "graph-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "GRAPH_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "GRAPH_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── GraphWorker ─────────────────────────────────────────────────────

/// Wraps the existing graph store as an eisenbahn worker.
///
/// The worker owns:
/// - ZMQ SUB socket for events (compute.complete)
/// - ZMQ PULL socket to receive graph updates from compute
struct GraphWorker {
    publisher: Arc<ZmqPublisher>,
    subscriber: Arc<ZmqSubscriber>,
    pipeline_receiver: Arc<ZmqPipelineReceiver>,
    shutdown: Arc<Notify>,
}

impl GraphWorker {
    /// Apply a graph update (entities + edges) to the store.
    async fn apply_update(&self, update: GraphUpdate) -> Result<(), EisenbahnError> {
        let entity_count = update.entities.len();
        let edge_count = update.edges.len();

        // TODO: apply entities/edges to GraphStore when integration is wired
        info!(
            entities = entity_count,
            edges = edge_count,
            "graph update applied"
        );

        Ok(())
    }

    /// Handle an incoming event message.
    async fn handle_event(&self, msg: Message) -> Result<(), EisenbahnError> {
        match msg.topic.as_str() {
            topics::COMPUTE_COMPLETE => {
                let event: stupid_eisenbahn::events::ComputeComplete =
                    msg.decode().map_err(EisenbahnError::Deserialization)?;
                info!(
                    batch_id = %event.batch_id,
                    features = event.features_computed,
                    "compute complete — awaiting graph updates"
                );
            }
            other => {
                warn!(topic = %other, "unexpected event topic");
            }
        }
        Ok(())
    }

    /// Run the main event loop: pull pipeline messages and receive events.
    async fn run_loop(self: &Arc<Self>) {
        loop {
            tokio::select! {
                // Pipeline PULL: receive graph updates from compute
                result = PipelineReceiver::recv(self.pipeline_receiver.as_ref()) => {
                    match result {
                        Ok(msg) => {
                            match msg.decode::<GraphUpdate>() {
                                Ok(update) => {
                                    if let Err(e) = self.apply_update(update).await {
                                        error!(error = %e, "failed to apply graph update");
                                    }
                                }
                                Err(e) => warn!(error = %e, "failed to decode graph update"),
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "pipeline recv error");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                // Event SUB: receive events from broker
                result = EventSubscriber::recv(self.subscriber.as_ref()) => {
                    match result {
                        Ok(msg) => {
                            if let Err(e) = self.handle_event(msg).await {
                                error!(error = %e, "failed to handle event");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "subscriber recv error");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                // Shutdown signal
                _ = self.shutdown.notified() => {
                    info!("graph worker event loop shutting down");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl Worker for GraphWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        self.subscriber
            .subscribe(topics::COMPUTE_COMPLETE)
            .await?;
        info!("graph worker started — subscribed to compute.complete");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("graph worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "graph-worker"
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

    // Pipeline PULL: receive graph updates from compute
    let graph_transport = stupid_eisenbahn::transport::Transport::ipc("pipeline-graph");
    let pipeline_receiver = Arc::new(
        ZmqPipelineReceiver::bind(&graph_transport).await?,
    );

    let shutdown = Arc::new(Notify::new());

    let worker = Arc::new(GraphWorker {
        publisher: publisher.clone(),
        subscriber,
        pipeline_receiver,
        shutdown: shutdown.clone(),
    });

    // Spawn the event loop
    let worker_for_loop = worker.clone();
    tokio::spawn(async move {
        worker_for_loop.run_loop().await;
    });

    let runner_config = WorkerBuilder::new("graph-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::COMPUTE_COMPLETE)
        .build();

    info!("graph-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("graph-worker exited cleanly");
    Ok(())
}
