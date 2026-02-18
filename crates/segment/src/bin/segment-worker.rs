//! segment-worker — Eisenbahn worker managing segment storage.
//!
//! Subscribes to events:
//! - `eisenbahn.ingest.complete` — triggers segment rotation checks
//!
//! Publishes events:
//! - `eisenbahn.worker.health` — periodic health pings

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::info;

use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, Worker, WorkerBuilder, WorkerRunner, ZmqPublisher,
};
use stupid_eisenbahn::topics;

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn segment worker — manages time-partitioned segment storage.
#[derive(Parser, Debug)]
#[command(name = "segment-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "SEGMENT_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "SEGMENT_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── SegmentWorker ───────────────────────────────────────────────────

struct SegmentWorker {
    shutdown: Arc<Notify>,
}

#[async_trait]
impl Worker for SegmentWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("segment worker started");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("segment worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "segment-worker"
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

    let shutdown = Arc::new(Notify::new());

    let worker = Arc::new(SegmentWorker {
        shutdown: shutdown.clone(),
    });

    let runner_config = WorkerBuilder::new("segment-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::INGEST_COMPLETE)
        .build();

    info!("segment-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("segment-worker exited cleanly");

    Ok(())
}
