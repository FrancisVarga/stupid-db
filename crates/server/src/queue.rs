//! Background SQS queue consumer task.
//!
//! Polls SQS for raw event messages, accumulates them into micro-batches,
//! parses into Documents, applies graph extraction, runs the compute
//! pipeline, and broadcasts updates to WebSocket clients.

use std::sync::Arc;

use tracing::{error, info, warn};

use stupid_core::Config;
use stupid_queue::{MicroBatcher, QueueConsumer, SqsConsumer, parse_batch};

use crate::state::AppState;

/// Background task: poll SQS → micro-batch → parse → graph + pipeline.
///
/// Waits for initial data loading to complete before starting consumption,
/// so graph and compute state are fully initialized.
pub async fn queue_ingest(config: &Config, app_state: Arc<AppState>) {
    if !config.queue.enabled {
        info!("Queue consumer disabled (QUEUE_ENABLED=false)");
        return;
    }

    if config.queue.queue_url.is_empty() {
        warn!("Queue enabled but QUEUE_URL is empty — skipping queue consumer");
        return;
    }

    // Wait for initial data load to finish before consuming queue messages.
    // Graph algorithms and pipeline need the full historical dataset first.
    info!("Queue consumer waiting for initial data load...");
    loop {
        if app_state.loading.is_ready().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Create SQS consumer with queue-specific AWS credentials (QUEUE_AWS_*).
    let consumer = match SqsConsumer::new(&config.queue.aws, &config.queue).await {
        Ok(c) => {
            info!(queue_url = %config.queue.queue_url, "SQS consumer connected");
            c
        }
        Err(e) => {
            error!("Failed to create SQS consumer: {} — queue ingestion disabled", e);
            return;
        }
    };

    let mut batcher = MicroBatcher::new(
        config.queue.micro_batch_size,
        std::time::Duration::from_millis(config.queue.micro_batch_timeout_ms),
    );

    let poll_interval = std::time::Duration::from_millis(config.queue.poll_interval_ms);

    info!(
        poll_interval_ms = config.queue.poll_interval_ms,
        micro_batch_size = config.queue.micro_batch_size,
        micro_batch_timeout_ms = config.queue.micro_batch_timeout_ms,
        "Queue consumer started"
    );

    loop {
        // Poll SQS for messages.
        match consumer.poll_batch(config.queue.max_batch_size).await {
            Ok(messages) if !messages.is_empty() => {
                batcher.push(messages);
            }
            Ok(_) => {
                // Empty poll — check time-based flush before sleeping.
            }
            Err(e) => {
                warn!("SQS poll error: {} — retrying in {:?}", e, poll_interval);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        }

        // Flush if size or time threshold is met.
        if let Some(batch) = batcher.try_flush() {
            process_batch(&batch, &consumer, &app_state).await;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Process a flushed micro-batch: parse → graph → pipeline → ack/nack.
async fn process_batch(
    messages: &[stupid_queue::QueueMessage],
    consumer: &SqsConsumer,
    app_state: &Arc<AppState>,
) {
    let batch_start = std::time::Instant::now();

    // Parse JSON bodies into Documents.
    let (docs, errors) = parse_batch(messages);

    // Nack unparseable messages so SQS retries / routes to DLQ.
    for (msg_id, err) in &errors {
        warn!(message_id = %msg_id, error = %err, "Nacking unparseable message");
        if let Some(handle) = messages.iter()
            .find(|m| m.id == *msg_id)
            .map(|m| &m.receipt_handle)
        {
            if let Err(e) = consumer.nack(handle).await {
                warn!(message_id = %msg_id, "Failed to nack: {}", e);
            }
        }
    }

    if docs.is_empty() {
        return;
    }

    // Extract graph ops and apply to the shared graph.
    let graph_ops_count = {
        let mut all_ops = Vec::new();
        for doc in &docs {
            crate::extract_graph_ops(doc, "queue", &mut all_ops);
        }

        let mut graph = app_state.graph.write().await;
        for op in &all_ops {
            crate::apply_graph_op(op, &mut graph, "queue");
        }
        all_ops.len()
    };

    // Run compute pipeline (hot_connect) on the new documents.
    {
        let mut pipe = app_state.pipeline.lock().unwrap();
        let mut knowledge = app_state.knowledge.write().unwrap();
        pipe.hot_connect(&docs, &mut knowledge);
    }

    // Update doc count.
    app_state
        .doc_count
        .fetch_add(docs.len() as u64, std::sync::atomic::Ordering::Relaxed);

    // Broadcast update to WebSocket clients.
    let _ = app_state.broadcast.send(
        serde_json::json!({
            "type": "queue_batch",
            "docs": docs.len(),
            "graph_ops": graph_ops_count,
        })
        .to_string(),
    );

    // Ack all messages whose IDs are NOT in the error list.
    let error_ids: std::collections::HashSet<&str> = errors.iter().map(|(id, _)| id.as_str()).collect();
    for msg in messages {
        if !error_ids.contains(msg.id.as_str()) {
            if let Err(e) = consumer.ack(&msg.receipt_handle).await {
                warn!(message_id = %msg.id, "Failed to ack: {}", e);
            }
        }
    }

    info!(
        docs = docs.len(),
        graph_ops = graph_ops_count,
        errors = errors.len(),
        elapsed_ms = batch_start.elapsed().as_millis() as u64,
        "Queue batch processed"
    );
}
