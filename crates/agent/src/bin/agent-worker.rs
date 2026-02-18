//! agent-worker — Eisenbahn worker managing agentic loops.
//!
//! Subscribes to events:
//! - `eisenbahn.compute.complete` — triggers agent reasoning on new results
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
use stupid_eisenbahn::services::{AgentServiceRequest, ServiceError};
use stupid_eisenbahn::topics;

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn agent worker — manages agentic reasoning loops.
#[derive(Parser, Debug)]
#[command(name = "agent-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "AGENT_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "AGENT_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── AgentWorker ─────────────────────────────────────────────────────

struct AgentWorker {
    shutdown: Arc<Notify>,
    request_server: Option<Arc<ZmqRequestServer>>,
}

#[async_trait]
impl Worker for AgentWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        info!("agent worker started");

        if let Some(server) = &self.request_server {
            let server = server.clone();
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                loop {
                    tokio::select! {
                        result = server.recv_request() => {
                            match result {
                                Ok((token, msg)) => {
                                    info!(topic = %msg.topic, correlation_id = %msg.correlation_id, "received agent service request");
                                    match msg.decode::<AgentServiceRequest>() {
                                        Ok(req) => {
                                            let variant = match &req {
                                                AgentServiceRequest::Execute { agent_name, .. } => {
                                                    format!("Execute({})", agent_name)
                                                }
                                                AgentServiceRequest::ExecuteWithHistory { agent_name, .. } => {
                                                    format!("ExecuteWithHistory({})", agent_name)
                                                }
                                                AgentServiceRequest::ExecuteDirect { .. } => {
                                                    "ExecuteDirect".to_string()
                                                }
                                                AgentServiceRequest::TeamExecute { strategy, .. } => {
                                                    format!("TeamExecute({})", strategy)
                                                }
                                            };
                                            info!(variant = %variant, "agent request");
                                            // TODO: Wire actual agent execution engine
                                            let error = ServiceError {
                                                code: 503,
                                                message: "agent execution not yet wired".into(),
                                            };
                                            let reply = Message::with_correlation(
                                                topics::SVC_AGENT_RESPONSE,
                                                &error,
                                                msg.correlation_id,
                                            ).unwrap();
                                            if let Err(e) = server.send_reply(token, reply).await {
                                                warn!(error = %e, "failed to send agent reply");
                                            }
                                        }
                                        Err(e) => {
                                            warn!(error = %e, "failed to decode agent request");
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "agent request server recv error");
                                    tokio::time::sleep(Duration::from_millis(100)).await;
                                }
                            }
                        }
                        _ = shutdown.notified() => {
                            info!("agent request handler shutting down");
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
        info!("agent worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "agent-worker"
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

    let request_server = if let Some(transport) = config.service_transport("agent") {
        info!(endpoint = %transport.endpoint(), "binding agent service ROUTER socket");
        Some(Arc::new(ZmqRequestServer::bind(&transport).await?))
    } else {
        info!("no agent service endpoint configured — request handling disabled");
        None
    };

    let worker = Arc::new(AgentWorker {
        shutdown: shutdown.clone(),
        request_server,
    });

    let runner_config = WorkerBuilder::new("agent-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::COMPUTE_COMPLETE)
        .build();

    info!("agent-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("agent-worker exited cleanly");

    Ok(())
}
