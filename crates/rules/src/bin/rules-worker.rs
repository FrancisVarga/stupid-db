//! rules-worker — Eisenbahn worker wrapping the rules engine.
//!
//! Watches the rules directory for changes and publishes:
//! - `eisenbahn.rule.changed` — when a rule is created, updated, or deleted
//!
//! Subscribes to events:
//! - `eisenbahn.ingest.complete` — triggers rule re-evaluation on new data
//!
//! Other workers subscribe to rule.changed to reload their configs.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::Notify;
use tracing::{error, info, warn};

use stupid_eisenbahn::events::{IngestComplete, RuleAction, RuleChanged};
use stupid_eisenbahn::topics;
use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventPublisher, EventSubscriber, Message, Worker,
    WorkerBuilder, WorkerRunner, ZmqPublisher, ZmqSubscriber,
};

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn rules worker — rule file watching and change notifications.
#[derive(Parser, Debug)]
#[command(name = "rules-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Path to the rules directory to watch.
    #[arg(long, env = "RULES_DIR", default_value = "data/rules")]
    rules_dir: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "RULES_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "RULES_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── RulesWorker ─────────────────────────────────────────────────────

/// Wraps the existing rules engine as an eisenbahn worker.
///
/// Watches the rules directory via `notify` crate (already a dep of stupid-rules)
/// and publishes rule.changed events when YAML files are modified.
struct RulesWorker {
    publisher: Arc<ZmqPublisher>,
    subscriber: Arc<ZmqSubscriber>,
    #[allow(dead_code)]
    rules_dir: String,
    shutdown: Arc<Notify>,
}

impl RulesWorker {
    /// Publish a rule.changed event.
    #[allow(dead_code)]
    async fn publish_rule_changed(&self, rule_id: &str, action: RuleAction) {
        let event = RuleChanged {
            rule_id: rule_id.to_string(),
            action,
        };
        match Message::new(topics::RULE_CHANGED, &event) {
            Ok(msg) => {
                if let Err(e) = self.publisher.publish(msg).await {
                    warn!(error = %e, "failed to publish rule.changed");
                }
            }
            Err(e) => warn!(error = %e, "failed to serialize rule.changed"),
        }
    }

    /// Handle an incoming event message.
    async fn handle_event(&self, msg: Message) -> Result<(), EisenbahnError> {
        match msg.topic.as_str() {
            topics::INGEST_COMPLETE => {
                let event: IngestComplete =
                    msg.decode().map_err(EisenbahnError::Deserialization)?;
                info!(
                    source = %event.source,
                    records = event.record_count,
                    "ingest complete — rule re-evaluation pending"
                );
                // TODO: trigger rule re-evaluation on newly ingested data
            }
            other => {
                warn!(topic = %other, "unexpected event topic");
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
                                error!(error = %e, "failed to handle event");
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "subscriber recv error");
                            tokio::time::sleep(Duration::from_millis(100)).await;
                        }
                    }
                }
                _ = self.shutdown.notified() => {
                    info!("rules worker event loop shutting down");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl Worker for RulesWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        self.subscriber
            .subscribe(topics::INGEST_COMPLETE)
            .await?;
        // TODO: start file watcher on rules_dir using notify crate
        info!(rules_dir = %self.rules_dir, "rules worker started — subscribed to ingest.complete");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("rules worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "rules-worker"
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

    let worker = Arc::new(RulesWorker {
        publisher: publisher.clone(),
        subscriber,
        rules_dir: cli.rules_dir,
        shutdown: shutdown.clone(),
    });

    // Spawn the event loop
    let worker_for_loop = worker.clone();
    tokio::spawn(async move {
        worker_for_loop.run_loop().await;
    });

    let runner_config = WorkerBuilder::new("rules-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::INGEST_COMPLETE)
        .build();

    info!("rules-worker starting");
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;
    info!("rules-worker exited cleanly");
    Ok(())
}
