//! catalog-worker — Eisenbahn worker serving catalog queries.
//!
//! Subscribes to events:
//! - `eisenbahn.rule.changed` — refreshes catalog entries on rule updates
//!
//! Publishes events:
//! - `eisenbahn.worker.health` — periodic health pings

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::{info, warn};

use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, Message, RequestHandler, Worker, WorkerBuilder, WorkerRunner,
    ZmqPublisher, ZmqRequestServer,
};
use stupid_eisenbahn::services::{CatalogQueryRequest, ServiceError};
use stupid_eisenbahn::topics;

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn catalog worker — serves schema registry and catalog queries.
#[derive(Parser, Debug)]
#[command(name = "catalog-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "CATALOG_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "CATALOG_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── CatalogWorker ───────────────────────────────────────────────────

struct CatalogWorker {
    shutdown: Arc<Notify>,
    request_server: Option<Arc<ZmqRequestServer>>,
}

#[async_trait]
impl Worker for CatalogWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("catalog worker started");

        if let Some(server) = &self.request_server {
            let server = server.clone();
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        result = server.recv_request() => {
                            match result {
                                Ok((token, msg)) => {
                                    info!(topic = %msg.topic, correlation_id = %msg.correlation_id, "received catalog service request");
                                    match msg.decode::<CatalogQueryRequest>() {
                                        Ok(req) => {
                                            info!(steps = req.steps.len(), "catalog query with {} steps", req.steps.len());
                                            // TODO: Wire actual catalog::QueryExecutor when graph is available
                                            let error = ServiceError {
                                                code: 503,
                                                message: "catalog query execution not yet wired".into(),
                                            };
                                            let reply = Message::with_correlation(
                                                topics::SVC_CATALOG_RESPONSE,
                                                &error,
                                                msg.correlation_id,
                                            ).unwrap();
                                            if let Err(e) = server.send_reply(token, reply).await {
                                                warn!(error = %e, "failed to send catalog reply");
                                            }
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "failed to decode catalog request");
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "catalog request server recv error");
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
                            }
                        }
                        _ = shutdown.notified() => {
                            info!("catalog request handler shutting down");
                            break;
                        }
                    }
                }
            });
        }

        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("catalog worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "catalog-worker"
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

    let request_server = if let Some(transport) = config.service_transport("catalog") {
        info!(endpoint = %transport.endpoint(), "binding catalog service ROUTER socket");
        Some(Arc::new(ZmqRequestServer::bind(&transport).await?))
    } else {
        info!("no catalog service endpoint configured — request handling disabled");
        None
    };

    let worker = Arc::new(CatalogWorker {
        shutdown: shutdown.clone(),
        request_server,
    });

    let runner_config = WorkerBuilder::new("catalog-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::RULE_CHANGED)
        .build();

    info!("catalog-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("catalog-worker exited cleanly");

    Ok(())
}
