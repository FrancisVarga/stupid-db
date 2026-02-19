//! ingest-worker — Eisenbahn worker wrapping the ingest pipeline.
//!
//! Receives raw data (file watch events, API pushes) and transforms them
//! into Documents via the existing parquet/document importers.
//!
//! Pipeline flow: data sources → ingest → PUSH to compute
//!
//! Publishes events:
//! - `eisenbahn.ingest.complete` — after each batch is ingested

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::info;

use stupid_eisenbahn::events::IngestComplete;
use stupid_eisenbahn::topics;
use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventPublisher, Message, PipelineConfig, Worker,
    WorkerBuilder, WorkerRunner, ZmqPipelineSender, ZmqPublisher,
};

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn ingest worker — parquet/document ingestion and normalization.
#[derive(Parser, Debug)]
#[command(name = "ingest-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "INGEST_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "INGEST_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── IngestWorker ────────────────────────────────────────────────────

/// Wraps the existing ingest library as an eisenbahn worker.
///
/// The worker owns:
/// - ZMQ PUB socket for events (ingest.complete)
/// - ZMQ PUSH socket to feed batches downstream to compute
struct IngestWorker {
    publisher: Arc<ZmqPublisher>,
    pipeline_sender: Arc<ZmqPipelineSender>,
    shutdown: Arc<Notify>,
}

impl IngestWorker {
    /// Publish an ingest.complete event after processing a batch.
    async fn publish_complete(&self, source: &str, record_count: u64, duration_ms: u64) {
        let event = IngestComplete {
            source: source.to_string(),
            record_count,
            duration_ms,
            job_id: None,
            total_segments: 0,
            error: None,
            source_type: None,
        };
        match Message::new(topics::INGEST_COMPLETE, &event) {
            Ok(msg) => {
                if let Err(e) = self.publisher.publish(msg).await {
                    tracing::warn!(error = %e, "failed to publish ingest.complete");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to serialize ingest.complete"),
        }
    }
}

#[async_trait]
impl Worker for IngestWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("ingest worker started");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("ingest worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "ingest-worker"
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

    // Create ZMQ sockets
    let publisher: Arc<ZmqPublisher> = Arc::new(
        ZmqPublisher::connect(&config.broker_frontend_transport()).await?,
    );

    // Pipeline PUSH: send batches downstream to compute
    let compute_transport = stupid_eisenbahn::transport::Transport::ipc("pipeline-compute");
    let pipeline_sender = Arc::new(
        ZmqPipelineSender::new(&compute_transport, PipelineConfig::default()).await?,
    );

    let shutdown = Arc::new(Notify::new());

    let worker = Arc::new(IngestWorker {
        publisher: publisher.clone(),
        pipeline_sender,
        shutdown: shutdown.clone(),
    });

    let runner_config = WorkerBuilder::new("ingest-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .build();

    info!("ingest-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("ingest-worker exited cleanly");
    Ok(())
}
