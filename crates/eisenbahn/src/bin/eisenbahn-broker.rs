//! eisenbahn-broker — Central PUB/SUB event broker for the stupid-db messaging layer.
//!
//! Proxies messages from publishers (SUB frontend) to subscribers (PUB backend)
//! while collecting per-topic metrics. Provides a REP health check socket.
//!
//! # Usage
//!
//! ```bash
//! # Local IPC (default)
//! eisenbahn-broker
//!
//! # TCP with custom ports
//! eisenbahn-broker --transport tcp --host 0.0.0.0 --frontend-port 5555 --backend-port 5556 --health-port 5557
//!
//! # Via environment variables
//! EISENBAHN_TRANSPORT=tcp EISENBAHN_HOST=0.0.0.0 eisenbahn-broker
//! ```

use std::sync::Arc;

use clap::Parser;
use stupid_eisenbahn::broker::{BrokerConfig, EventBroker};
use stupid_eisenbahn::transport::Transport;

/// Central PUB/SUB event broker for the eisenbahn messaging layer.
#[derive(Parser, Debug)]
#[command(name = "eisenbahn-broker", version, about)]
struct Cli {
    /// Transport type: "ipc" or "tcp".
    #[arg(long, env = "EISENBAHN_TRANSPORT", default_value = "ipc")]
    transport: String,

    /// TCP host to bind to (only used with --transport tcp).
    #[arg(long, env = "EISENBAHN_HOST", default_value = "0.0.0.0")]
    host: String,

    /// Frontend port — publishers connect here (only used with --transport tcp).
    #[arg(long, env = "EISENBAHN_FRONTEND_PORT", default_value_t = 5555)]
    frontend_port: u16,

    /// Backend port — subscribers connect here (only used with --transport tcp).
    #[arg(long, env = "EISENBAHN_BACKEND_PORT", default_value_t = 5556)]
    backend_port: u16,

    /// Health check port (only used with --transport tcp).
    #[arg(long, env = "EISENBAHN_HEALTH_PORT", default_value_t = 5557)]
    health_port: u16,

    /// IPC socket name prefix (only used with --transport ipc).
    #[arg(long, env = "EISENBAHN_IPC_PREFIX", default_value = "broker")]
    ipc_prefix: String,

    /// HTTP port for the `/metrics` JSON endpoint (0 = disabled).
    #[arg(long, env = "EISENBAHN_METRICS_PORT", default_value_t = 0)]
    metrics_port: u16,

    /// Interval in seconds between metrics log lines (0 = disabled).
    #[arg(long, env = "EISENBAHN_METRICS_INTERVAL", default_value_t = 30)]
    metrics_interval: u64,
}

impl Cli {
    fn into_broker_config(self) -> BrokerConfig {
        let metrics_port = if self.metrics_port > 0 {
            Some(self.metrics_port)
        } else {
            None
        };
        match self.transport.as_str() {
            "tcp" => {
                let mut cfg = BrokerConfig::tcp(
                    &self.host,
                    self.frontend_port,
                    self.backend_port,
                    self.health_port,
                );
                cfg.metrics_port = metrics_port;
                cfg
            }
            _ => BrokerConfig {
                frontend: Transport::ipc(&format!("{}-frontend", self.ipc_prefix)),
                backend: Transport::ipc(&format!("{}-backend", self.ipc_prefix)),
                health: Transport::ipc(&format!("{}-health", self.ipc_prefix)),
                metrics_port,
            },
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Initialize structured logging.
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let cli = Cli::parse();
    let metrics_interval = cli.metrics_interval;

    tracing::info!(?cli, "starting eisenbahn-broker");

    let config = cli.into_broker_config();
    let broker = Arc::new(EventBroker::new(config));

    // Install signal handlers for graceful shutdown.
    let broker_for_signal = broker.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        tracing::info!("shutdown signal received");
        broker_for_signal.shutdown();
    });

    // Periodic metrics reporter.
    if metrics_interval > 0 {
        let metrics = broker.metrics().clone();
        tokio::spawn(async move {
            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(metrics_interval));
            loop {
                interval.tick().await;
                let total = metrics.total();
                let counts = metrics.topic_counts.lock().await;
                tracing::info!(
                    total_messages = total,
                    unique_topics = counts.len(),
                    "broker metrics"
                );
                if !counts.is_empty() {
                    for (topic, count) in counts.iter() {
                        tracing::debug!(topic = %topic, count = count, "topic stats");
                    }
                }
            }
        });
    }

    // Run the broker (blocks until shutdown).
    broker.run().await?;

    tracing::info!("eisenbahn-broker exited cleanly");
    Ok(())
}

/// Wait for SIGINT or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to register SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {},
            _ = sigterm.recv() => {},
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.expect("failed to listen for ctrl_c");
    }
}
