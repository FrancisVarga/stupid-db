//! athena-worker — Eisenbahn worker managing AWS Athena queries.
//!
//! Subscribes to events:
//! - `eisenbahn.ingest.complete` — triggers Athena query scheduling
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
use stupid_eisenbahn::services::{AthenaServiceRequest, ServiceError};
use stupid_eisenbahn::topics;

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn Athena worker — manages AWS Athena query execution.
#[derive(Parser, Debug)]
#[command(name = "athena-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "ATHENA_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "ATHENA_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── AthenaWorker ────────────────────────────────────────────────────

struct AthenaWorker {
    shutdown: Arc<Notify>,
    request_server: Option<Arc<ZmqRequestServer>>,
}

#[async_trait]
impl Worker for AthenaWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("athena worker started");

        if let Some(server) = &self.request_server {
            let server = server.clone();
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        result = server.recv_request() => {
                            match result {
                                Ok((token, msg)) => {
                                    info!(topic = %msg.topic, correlation_id = %msg.correlation_id, "received athena service request");
                                    match msg.decode::<AthenaServiceRequest>() {
                                        Ok(req) => {
                                            let variant = match &req {
                                                AthenaServiceRequest::Query { connection_id, sql, .. } => {
                                                    format!("Query({}, {})", connection_id, sql)
                                                }
                                                AthenaServiceRequest::QueryParquet { connection_id, sql, .. } => {
                                                    format!("QueryParquet({}, {})", connection_id, sql)
                                                }
                                                AthenaServiceRequest::SchemaRefresh { connection_id } => {
                                                    format!("SchemaRefresh({})", connection_id)
                                                }
                                            };
                                            info!(variant = %variant, "athena request");
                                            // TODO: Wire actual Athena SDK — streaming replies will come later
                                            let error = ServiceError {
                                                code: 503,
                                                message: "athena query execution not yet wired".into(),
                                            };
                                            let reply = Message::with_correlation(
                                                topics::SVC_ATHENA_RESPONSE,
                                                &error,
                                                msg.correlation_id,
                                            ).unwrap();
                                            if let Err(e) = server.send_reply(token, reply).await {
                                                warn!(error = %e, "failed to send athena reply");
                                            }
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "failed to decode athena request");
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "athena request server recv error");
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
                            }
                        }
                        _ = shutdown.notified() => {
                            info!("athena request handler shutting down");
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
        info!("athena worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "athena-worker"
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

    let request_server = if let Some(transport) = config.service_transport("athena") {
        info!(endpoint = %transport.endpoint(), "binding athena service ROUTER socket");
        Some(Arc::new(ZmqRequestServer::bind(&transport).await?))
    } else {
        info!("no athena service endpoint configured — request handling disabled");
        None
    };

    let worker = Arc::new(AthenaWorker {
        shutdown: shutdown.clone(),
        request_server,
    });

    let runner_config = WorkerBuilder::new("athena-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::INGEST_COMPLETE)
        .build();

    info!("athena-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("athena-worker exited cleanly");

    Ok(())
}
