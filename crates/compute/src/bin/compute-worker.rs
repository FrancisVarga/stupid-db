//! compute-worker — Eisenbahn worker wrapping the compute pipeline.
//!
//! Subscribes to events:
//! - `eisenbahn.ingest.complete` — triggers warm compute on the latest batch
//! - `eisenbahn.rule.changed` — reloads rule configs (placeholder)
//!
//! Pipeline flow: PULL from ingest → process → PUSH to graph
//!
//! Publishes events:
//! - `eisenbahn.compute.complete` — after each batch is processed
//! - `eisenbahn.anomaly.detected` — when anomaly scores exceed threshold

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use clap::Parser;
use tokio::sync::{Mutex, Notify};
use tracing::{error, info, warn};

use stupid_compute::pipeline::Pipeline;
use stupid_compute::scheduler::state::KnowledgeState;

use stupid_eisenbahn::events::{AnomalyDetected, ComputeComplete, IngestComplete, RuleChanged};
use stupid_eisenbahn::msg_pipeline::{GraphUpdate, IngestBatch};
use stupid_eisenbahn::topics;
use stupid_eisenbahn::{
    EisenbahnConfig, EisenbahnError, EventPublisher, EventSubscriber, Message, PipelineConfig,
    PipelineReceiver, PipelineSender, Worker, WorkerBuilder, WorkerRunner, ZmqPipelineReceiver,
    ZmqPipelineSender, ZmqPublisher, ZmqSubscriber,
};

use stupid_core::{Document, FieldValue};

// ── CLI ─────────────────────────────────────────────────────────────

/// Eisenbahn compute worker — feature extraction, anomaly detection, and trend analysis.
#[derive(Parser, Debug)]
#[command(name = "compute-worker", version, about)]
struct Cli {
    /// Path to eisenbahn.toml config file.
    #[arg(long, env = "EISENBAHN_CONFIG", default_value = "config/eisenbahn.toml")]
    config: String,

    /// Health ping interval in seconds.
    #[arg(long, env = "COMPUTE_HEALTH_INTERVAL", default_value_t = 30)]
    health_interval: u64,

    /// Shutdown timeout in seconds.
    #[arg(long, env = "COMPUTE_SHUTDOWN_TIMEOUT", default_value_t = 10)]
    shutdown_timeout: u64,
}

// ── ComputeWorker ───────────────────────────────────────────────────

/// Wraps the existing compute `Pipeline` as an eisenbahn worker.
///
/// The worker owns:
/// - A `Pipeline` for hot-path feature extraction and warm-path analysis
/// - A `KnowledgeState` that accumulates computed results
/// - ZMQ sockets for pipeline (PULL/PUSH) and event (PUB/SUB) messaging
struct ComputeWorker {
    pipeline: Mutex<Pipeline>,
    state: Mutex<KnowledgeState>,
    publisher: Arc<ZmqPublisher>,
    subscriber: Arc<ZmqSubscriber>,
    pipeline_receiver: Arc<ZmqPipelineReceiver>,
    pipeline_sender: Arc<ZmqPipelineSender>,
    shutdown: Arc<Notify>,
}

impl ComputeWorker {
    /// Process an ingest batch: convert pipeline records to Documents,
    /// run hot_connect, then push graph updates downstream.
    async fn process_batch(&self, batch: IngestBatch) -> Result<(), EisenbahnError> {
        let docs = Self::records_to_documents(&batch);
        let doc_count = docs.len();

        if docs.is_empty() {
            return Ok(());
        }

        // Run the hot path (feature extraction + streaming K-means)
        let anomaly_count = {
            let mut pipeline = self.pipeline.lock().await;
            let mut state = self.state.lock().await;
            pipeline.hot_connect(&docs, &mut state);

            // Run warm compute for anomaly detection and trend analysis
            pipeline.warm_compute(&mut state, &docs);

            // Collect anomalies to publish
            let anomalies: Vec<_> = state
                .anomalies
                .iter()
                .filter(|(_, score)| score.is_anomalous)
                .map(|(id, score)| (*id, score.score))
                .collect();

            // Publish anomaly events
            for (entity_id, score) in &anomalies {
                let event = AnomalyDetected {
                    rule_id: "compute-zscore".to_string(),
                    entity_id: entity_id.to_string(),
                    score: *score,
                };
                match Message::new(topics::ANOMALY_DETECTED, &event) {
                    Ok(msg) => {
                        if let Err(e) = self.publisher.publish(msg).await {
                            warn!(error = %e, "failed to publish anomaly event");
                        }
                    }
                    Err(e) => warn!(error = %e, "failed to serialize anomaly event"),
                }
            }

            anomalies.len()
        };

        // Push graph update downstream (entities/edges derived from features)
        let graph_update = GraphUpdate {
            entities: vec![],
            edges: vec![],
        };
        match Message::new(topics::GRAPH_UPDATE, &graph_update) {
            Ok(msg) => {
                if let Err(e) = self.pipeline_sender.send(msg).await {
                    warn!(error = %e, "failed to push graph update");
                }
            }
            Err(e) => warn!(error = %e, "failed to serialize graph update"),
        }

        // Publish compute.complete event
        let complete = ComputeComplete {
            batch_id: uuid::Uuid::new_v4().to_string(),
            features_computed: doc_count as u64,
        };
        match Message::new(topics::COMPUTE_COMPLETE, &complete) {
            Ok(msg) => {
                if let Err(e) = self.publisher.publish(msg).await {
                    warn!(error = %e, "failed to publish compute.complete");
                }
            }
            Err(e) => warn!(error = %e, "failed to serialize compute.complete"),
        }

        info!(
            docs = doc_count,
            anomalies = anomaly_count,
            "batch processed"
        );

        Ok(())
    }

    /// Handle an event message from the subscriber.
    async fn handle_event(&self, msg: Message) -> Result<(), EisenbahnError> {
        match msg.topic.as_str() {
            topics::INGEST_COMPLETE => {
                let event: IngestComplete = msg
                    .decode()
                    .map_err(EisenbahnError::Deserialization)?;
                info!(
                    source = %event.source,
                    records = event.record_count,
                    "ingest complete event received — ready for next batch"
                );
            }
            topics::RULE_CHANGED => {
                let event: RuleChanged = msg
                    .decode()
                    .map_err(EisenbahnError::Deserialization)?;
                info!(
                    rule_id = %event.rule_id,
                    action = ?event.action,
                    "rule changed — reload pending"
                );
                // TODO: reload rule configs when RuleLoader integration is added
            }
            other => {
                warn!(topic = %other, "unexpected event topic");
            }
        }
        Ok(())
    }

    /// Convert eisenbahn pipeline `Record`s into core `Document`s.
    fn records_to_documents(batch: &IngestBatch) -> Vec<Document> {
        batch
            .records
            .iter()
            .map(|record| {
                let mut fields = std::collections::HashMap::new();
                for (key, value) in &record.fields {
                    let fv = match value {
                        serde_json::Value::String(s) => FieldValue::Text(s.clone()),
                        serde_json::Value::Number(n) => {
                            if let Some(f) = n.as_f64() {
                                FieldValue::Float(f)
                            } else {
                                FieldValue::Text(n.to_string())
                            }
                        }
                        serde_json::Value::Bool(b) => FieldValue::Text(b.to_string()),
                        other => FieldValue::Text(other.to_string()),
                    };
                    fields.insert(key.clone(), fv);
                }

                let event_type = record
                    .fields
                    .get("eventType")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();

                Document {
                    id: uuid::Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    event_type,
                    fields,
                }
            })
            .collect()
    }

    /// Run the main event loop: pull pipeline messages and receive events concurrently.
    async fn run_loop(self: &Arc<Self>) {
        loop {
            tokio::select! {
                // Pipeline PULL: receive batches from ingest
                result = PipelineReceiver::recv(self.pipeline_receiver.as_ref()) => {
                    match result {
                        Ok(msg) => {
                            match msg.decode::<IngestBatch>() {
                                Ok(batch) => {
                                    if let Err(e) = self.process_batch(batch).await {
                                        error!(error = %e, "failed to process batch");
                                    }
                                }
                                Err(e) => warn!(error = %e, "failed to decode ingest batch"),
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
                    info!("compute worker event loop shutting down");
                    break;
                }
            }
        }
    }
}

#[async_trait]
impl Worker for ComputeWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        // Subscribe to relevant event topics
        self.subscriber.subscribe(topics::INGEST_COMPLETE).await?;
        self.subscriber.subscribe(topics::RULE_CHANGED).await?;
        info!("compute worker started — subscribed to events");
        Ok(())
    }

    async fn stop(&self) -> Result<(), EisenbahnError> {
        self.shutdown.notify_waiters();
        info!("compute worker stopped");
        Ok(())
    }

    fn name(&self) -> &str {
        "compute-worker"
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

    // Load eisenbahn config (fall back to local defaults if file not found)
    let config = match EisenbahnConfig::from_file(&cli.config) {
        Ok(cfg) => {
            info!(path = %cli.config, "loaded eisenbahn config");
            cfg
        }
        Err(e) => {
            warn!(
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
    let subscriber = Arc::new(
        ZmqSubscriber::connect(&config.broker_backend_transport()).await?,
    );

    // Pipeline sockets: PULL from ingest (bind), PUSH to graph (connect)
    let compute_transport = stupid_eisenbahn::transport::Transport::ipc("pipeline-compute");
    let graph_transport = stupid_eisenbahn::transport::Transport::ipc("pipeline-graph");

    let pipeline_receiver = Arc::new(
        ZmqPipelineReceiver::bind(&compute_transport).await?,
    );
    let pipeline_sender = Arc::new(
        ZmqPipelineSender::new(&graph_transport, PipelineConfig::default()).await?,
    );

    let shutdown = Arc::new(Notify::new());

    let worker = Arc::new(ComputeWorker {
        pipeline: Mutex::new(Pipeline::new()),
        state: Mutex::new(KnowledgeState::default()),
        publisher: publisher.clone(),
        subscriber,
        pipeline_receiver,
        pipeline_sender,
        shutdown: shutdown.clone(),
    });

    // Spawn the event loop
    let worker_for_loop = worker.clone();
    tokio::spawn(async move {
        worker_for_loop.run_loop().await;
    });

    // Build the worker runner config
    let runner_config = WorkerBuilder::new("compute-worker")
        .health_interval(Duration::from_secs(cli.health_interval))
        .shutdown_timeout(Duration::from_secs(cli.shutdown_timeout))
        .subscribe(topics::INGEST_COMPLETE)
        .subscribe(topics::RULE_CHANGED)
        .build();

    info!("compute-worker starting");

    // Run the worker (blocks until shutdown signal)
    WorkerRunner::run(worker, publisher, runner_config, Some(shutdown)).await?;

    info!("compute-worker exited cleanly");
    Ok(())
}
