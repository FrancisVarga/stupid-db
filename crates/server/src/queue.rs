//! Background SQS queue consumer tasks — store-driven.
//!
//! Reads enabled queue connections from `QueueConnectionStore`, spawns one
//! consumer per connection. Each consumer polls SQS, accumulates into
//! micro-batches, parses into Documents, persists to segments, applies graph
//! extraction, runs the compute pipeline, and broadcasts updates to WebSocket
//! clients.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;

use tracing::{error, info, warn};

use stupid_queue::{MicroBatcher, QueueConsumer, SqsConsumer, parse_batch};

use crate::queue_connections::QueueConnectionConfig;
use crate::state::{AppState, QueueMetrics};

/// Extract the queue name from the queue URL.
///
/// For SQS: `https://sqs.region.amazonaws.com/123456789/my-queue.fifo` → `my-queue.fifo`
/// Falls back to the full URL if no path segment is found.
fn queue_name_from_url(url: &str) -> String {
    url.rsplit('/')
        .next()
        .filter(|s| !s.is_empty())
        .unwrap_or(url)
        .to_string()
}

/// Spawn one SQS consumer per enabled queue connection in the store.
///
/// Waits for initial data loading to complete before starting consumption,
/// so graph and compute state are fully initialized.
pub async fn spawn_queue_consumers(app_state: Arc<AppState>) {
    // Wait for initial data load to finish before consuming queue messages.
    info!("Queue consumers waiting for initial data load...");
    loop {
        if app_state.loading.is_ready().await {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
    }

    // Read all queue connections from the encrypted store.
    let configs = {
        let store = app_state.queue_connections.read().await;
        match store.list_configs() {
            Ok(configs) => configs,
            Err(e) => {
                error!("Failed to read queue connections: {} — no consumers started", e);
                return;
            }
        }
    };

    let enabled: Vec<_> = configs.into_iter().filter(|c| c.enabled).collect();
    if enabled.is_empty() {
        info!("No enabled queue connections — queue consumers idle");
        return;
    }

    info!("Spawning {} queue consumer(s) from connection store", enabled.len());

    for config in enabled {
        let state = app_state.clone();
        tokio::spawn(async move {
            run_queue_consumer(config, state).await;
        });
    }
}

/// Run a single queue consumer for the given connection config.
///
/// Creates per-queue metrics, connects to SQS, and enters the poll loop.
async fn run_queue_consumer(config: QueueConnectionConfig, app_state: Arc<AppState>) {
    let queue_id = config.id.clone();
    let queue_name = config.name.clone();

    // Create per-queue metrics and register in the shared map.
    let metrics = Arc::new(QueueMetrics::new());
    metrics.enabled.store(true, Ordering::Relaxed);
    {
        let mut map = app_state.queue_metrics.write().unwrap();
        map.insert(queue_id.clone(), metrics.clone());
    }

    // Convert store config to the types SqsConsumer::new() expects.
    let aws_config = config.to_aws_config();
    let queue_config = config.to_queue_config();

    if queue_config.queue_url.is_empty() {
        warn!(
            queue_id = %queue_id,
            queue_name = %queue_name,
            "Queue URL is empty — skipping consumer"
        );
        return;
    }

    // Create SQS consumer with decrypted credentials.
    let consumer = match SqsConsumer::new(&aws_config, &queue_config).await {
        Ok(c) => {
            info!(
                queue_id = %queue_id,
                queue_name = %queue_name,
                queue_url = %queue_config.queue_url,
                "SQS consumer connected"
            );
            metrics.connected.store(true, Ordering::Relaxed);
            c
        }
        Err(e) => {
            error!(
                queue_id = %queue_id,
                queue_name = %queue_name,
                "Failed to create SQS consumer: {} — consumer disabled",
                e
            );
            return;
        }
    };

    // Compute the queue storage directory: data/{provider}/{queue_name}/
    let url_queue_name = queue_name_from_url(&queue_config.queue_url);
    let queue_base_dir = app_state.data_dir.join(&config.provider).join(&url_queue_name);
    info!(
        queue_dir = %queue_base_dir.display(),
        provider = %config.provider,
        queue_name = %url_queue_name,
        "Queue data will be stored at"
    );

    let mut batcher = MicroBatcher::new(
        queue_config.micro_batch_size,
        std::time::Duration::from_millis(queue_config.micro_batch_timeout_ms),
    );

    let poll_interval = std::time::Duration::from_millis(queue_config.poll_interval_ms);

    info!(
        queue_id = %queue_id,
        poll_interval_ms = queue_config.poll_interval_ms,
        micro_batch_size = queue_config.micro_batch_size,
        micro_batch_timeout_ms = queue_config.micro_batch_timeout_ms,
        "Queue consumer started"
    );

    loop {
        // Poll SQS for messages.
        match consumer.poll_batch(queue_config.max_batch_size).await {
            Ok(messages) if !messages.is_empty() => {
                metrics.messages_received.fetch_add(messages.len() as u64, Ordering::Relaxed);
                metrics.last_poll_epoch_ms.store(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    Ordering::Relaxed,
                );
                batcher.push(messages);
            }
            Ok(_) => {
                // Empty poll — update last poll time.
                metrics.last_poll_epoch_ms.store(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis() as u64,
                    Ordering::Relaxed,
                );
            }
            Err(e) => {
                warn!(
                    queue_id = %queue_id,
                    "SQS poll error: {} — retrying in {:?}", e, poll_interval
                );
                metrics.connected.store(false, Ordering::Relaxed);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        }

        metrics.connected.store(true, Ordering::Relaxed);

        // Flush if size or time threshold is met.
        if let Some(batch) = batcher.try_flush() {
            process_batch(&batch, &consumer, &app_state, &queue_base_dir, &metrics).await;
        }

        tokio::time::sleep(poll_interval).await;
    }
}

/// Persist parsed documents to a daily segment file under the queue directory.
fn persist_to_segment(
    docs: &[stupid_core::Document],
    app_state: &AppState,
    queue_base_dir: &PathBuf,
) {
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let mut writer_lock = app_state.queue_writer.lock().unwrap();

    let needs_new_writer = writer_lock
        .as_ref()
        .map(|(id, _)| id != &today)
        .unwrap_or(true);

    if needs_new_writer {
        if let Some((old_id, old_writer)) = writer_lock.take() {
            if let Err(e) = old_writer.finalize() {
                warn!(segment_id = %old_id, "Failed to finalize queue segment: {}", e);
            } else {
                info!(segment_id = %old_id, "Queue segment finalized (day rollover)");
            }
        }
        let segment_dir = queue_base_dir.join(&today);
        match stupid_segment::writer::SegmentWriter::new_at(segment_dir, &today) {
            Ok(w) => {
                info!(segment_id = %today, dir = %queue_base_dir.display(), "Created queue segment writer");
                *writer_lock = Some((today.clone(), w));
            }
            Err(e) => {
                warn!(segment_id = %today, "Failed to create queue segment writer: {}", e);
                return;
            }
        }
    }

    if let Some((_, ref mut writer)) = *writer_lock {
        for doc in docs {
            if let Err(e) = writer.append(doc) {
                warn!(doc_id = %doc.id, "Failed to write queue doc to segment: {}", e);
            }
        }
    }
}

/// Build a JSON summary of each document for WebSocket broadcast.
fn build_message_summaries(docs: &[stupid_core::Document]) -> Vec<serde_json::Value> {
    docs.iter()
        .map(|doc| {
            let fields: serde_json::Map<String, serde_json::Value> = doc
                .fields
                .iter()
                .map(|(k, v)| {
                    let json_val = match v {
                        stupid_core::document::FieldValue::Text(s) => serde_json::Value::String(s.clone()),
                        stupid_core::document::FieldValue::Integer(i) => serde_json::json!(*i),
                        stupid_core::document::FieldValue::Float(f) => serde_json::json!(*f),
                        stupid_core::document::FieldValue::Boolean(b) => serde_json::json!(*b),
                        stupid_core::document::FieldValue::Null => serde_json::Value::Null,
                    };
                    (k.clone(), json_val)
                })
                .collect();
            serde_json::json!({
                "event_type": doc.event_type,
                "id": doc.id.to_string(),
                "timestamp": doc.timestamp.to_rfc3339(),
                "fields": fields,
            })
        })
        .collect()
}

/// Process a flushed micro-batch: parse → persist → graph → pipeline → ack/nack.
async fn process_batch(
    messages: &[stupid_queue::QueueMessage],
    consumer: &SqsConsumer,
    app_state: &Arc<AppState>,
    queue_base_dir: &PathBuf,
    metrics: &Arc<QueueMetrics>,
) {
    let batch_start = std::time::Instant::now();

    let (docs, errors) = parse_batch(messages);

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

    persist_to_segment(&docs, app_state, queue_base_dir);

    let message_summaries = build_message_summaries(&docs);

    let graph_ops_count = {
        let mut all_ops = Vec::new();
        for doc in &docs {
            crate::graph_ops::extract_graph_ops(doc, "queue", &mut all_ops);
        }

        let mut graph = app_state.graph.write().await;
        for op in &all_ops {
            crate::graph_ops::apply_graph_op(op, &mut graph, "queue");
        }
        all_ops.len()
    };

    {
        let mut pipe = app_state.pipeline.lock().unwrap();
        let mut knowledge = app_state.knowledge.write().unwrap();
        pipe.hot_connect(&docs, &mut knowledge);
    }

    app_state
        .doc_count
        .fetch_add(docs.len() as u64, std::sync::atomic::Ordering::Relaxed);

    let _ = app_state.broadcast.send(
        serde_json::json!({
            "type": "queue_batch",
            "docs": docs.len(),
            "graph_ops": graph_ops_count,
            "messages": message_summaries,
        })
        .to_string(),
    );

    let error_ids: std::collections::HashSet<&str> = errors.iter().map(|(id, _)| id.as_str()).collect();
    for msg in messages {
        if !error_ids.contains(msg.id.as_str()) {
            if let Err(e) = consumer.ack(&msg.receipt_handle).await {
                warn!(message_id = %msg.id, "Failed to ack: {}", e);
            }
        }
    }

    let elapsed = batch_start.elapsed();
    metrics.messages_processed.fetch_add(docs.len() as u64, Ordering::Relaxed);
    metrics.messages_failed.fetch_add(errors.len() as u64, Ordering::Relaxed);
    metrics.batches_processed.fetch_add(1, Ordering::Relaxed);
    metrics.total_processing_time_us.fetch_add(elapsed.as_micros() as u64, Ordering::Relaxed);

    info!(
        docs = docs.len(),
        graph_ops = graph_ops_count,
        errors = errors.len(),
        elapsed_ms = elapsed.as_millis() as u64,
        "Queue batch processed"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_queue_name_from_sqs_url() {
        assert_eq!(
            queue_name_from_url("https://sqs.ap-northeast-1.amazonaws.com/726077843975/the-wall-alert-stream.fifo"),
            "the-wall-alert-stream.fifo"
        );
    }

    #[test]
    fn test_queue_name_from_simple_url() {
        assert_eq!(
            queue_name_from_url("https://example.com/my-queue"),
            "my-queue"
        );
    }

    #[test]
    fn test_queue_name_fallback() {
        assert_eq!(
            queue_name_from_url("my-queue"),
            "my-queue"
        );
    }
}
