//! notify-worker — Eisenbahn worker dispatching notifications (email/telegram/webhook).
//!
//! Subscribes to events:
//! - `eisenbahn.anomaly.detected` — triggers alert notifications
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

/// Eisenbahn notify worker — dispatches email, telegram, and webhook alerts.
#[derive(Parser, Debug)]
#[command(name = "notify-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "NOTIFY_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "NOTIFY_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── NotifyWorker ────────────────────────────────────────────────────

struct NotifyWorker {
    shutdown: Arc<Notify>,
}

#[async_trait]
impl Worker for NotifyWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("notify worker started");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("notify worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "notify-worker"
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

    let worker = Arc::new(NotifyWorker {
        shutdown: shutdown.clone(),
    });

    let runner_config = WorkerBuilder::new("notify-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::ANOMALY_DETECTED)
        .build();

    info!("notify-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("notify-worker exited cleanly");

    Ok(())
}
