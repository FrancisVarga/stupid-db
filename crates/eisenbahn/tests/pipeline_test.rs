//! Integration tests for PUSH/PULL pipeline transport.
//!
//! These tests verify load balancing across multiple PULL receivers,
//! batch sending, and multi-stage pipeline chaining.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

use tokio::time::timeout;

use stupid_eisenbahn::messages::pipeline::{
    ComputeResult, Feature, IngestBatch, Record,
};
use stupid_eisenbahn::{PipelineReceiver, PipelineSender};
use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{Message, PipelineConfig, ZmqPipelineReceiver, ZmqPipelineSender};

const TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(100);

#[tokio::test]
async fn load_balancing_across_three_workers() {
    let transport = Transport::tcp("127.0.0.1", 16200);
    let config = PipelineConfig::default();

    // One sender binds, three receivers connect
    let sender = ZmqPipelineSender::bind(&transport, config).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let rx1 = ZmqPipelineReceiver::connect(&transport).await.unwrap();
    let rx2 = ZmqPipelineReceiver::connect(&transport).await.unwrap();
    let rx3 = ZmqPipelineReceiver::connect(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let total = 30u32;
    for i in 0..total {
        let msg = Message::new("pipeline.work", &i).unwrap();
        sender.send(msg).await.unwrap();
    }

    // Collect results from all three workers
    let counts = Arc::new([
        AtomicU32::new(0),
        AtomicU32::new(0),
        AtomicU32::new(0),
    ]);

    let (tx, mut channel_rx) = tokio::sync::mpsc::channel::<u32>(60);

    let spawn_receiver = |rx: ZmqPipelineReceiver, id: u32, tx: tokio::sync::mpsc::Sender<u32>| {
        tokio::spawn(async move {
            loop {
                match timeout(Duration::from_millis(500), rx.recv()).await {
                    Ok(Ok(_)) => {
                        let _ = tx.send(id).await;
                    }
                    _ => break,
                }
            }
        })
    };

    let h1 = spawn_receiver(rx1, 0, tx.clone());
    let h2 = spawn_receiver(rx2, 1, tx.clone());
    let h3 = spawn_receiver(rx3, 2, tx.clone());
    drop(tx);

    let c = counts.clone();
    while let Some(id) = channel_rx.recv().await {
        c[id as usize].fetch_add(1, Ordering::Relaxed);
    }

    let _ = tokio::join!(h1, h2, h3);

    let c0 = counts[0].load(Ordering::Relaxed);
    let c1 = counts[1].load(Ordering::Relaxed);
    let c2 = counts[2].load(Ordering::Relaxed);

    assert_eq!(c0 + c1 + c2, total, "all messages should be received");
    assert!(c0 > 0, "worker 0 should receive some messages (got {c0})");
    assert!(c1 > 0, "worker 1 should receive some messages (got {c1})");
    assert!(c2 > 0, "worker 2 should receive some messages (got {c2})");
}

#[tokio::test]
async fn ingest_batch_through_pipeline() {
    let transport = Transport::tcp("127.0.0.1", 16210);
    let config = PipelineConfig::default();

    let receiver = ZmqPipelineReceiver::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let sender = ZmqPipelineSender::new(&transport, config).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Send a typed IngestBatch through the pipeline
    let batch = IngestBatch {
        records: vec![
            Record {
                id: "r-1".into(),
                fields: HashMap::from([
                    ("user".into(), serde_json::json!("alice")),
                    ("amount".into(), serde_json::json!(42.0)),
                ]),
            },
            Record {
                id: "r-2".into(),
                fields: HashMap::from([("user".into(), serde_json::json!("bob"))]),
            },
        ],
    };

    let msg = Message::new("pipeline.ingest", &batch).unwrap();
    sender.send(msg).await.unwrap();

    let received = timeout(TIMEOUT, receiver.recv())
        .await
        .expect("timed out")
        .unwrap();

    let decoded: IngestBatch = received.decode().unwrap();
    assert_eq!(decoded.records.len(), 2);
    assert_eq!(decoded.records[0].id, "r-1");
    assert_eq!(decoded.records[1].id, "r-2");
}

#[tokio::test]
async fn two_stage_pipeline_chaining() {
    // Simulate: ingest stage → compute stage
    // PUSH(ingest) → PULL(compute_in) → transform → PUSH(compute_out) → PULL(graph_in)

    let stage1_transport = Transport::tcp("127.0.0.1", 16220);
    let stage2_transport = Transport::tcp("127.0.0.1", 16221);

    // Stage 1 receiver binds (ingest worker)
    let stage1_rx = ZmqPipelineReceiver::bind(&stage1_transport).await.unwrap();
    // Stage 2 receiver binds (graph worker)
    let stage2_rx = ZmqPipelineReceiver::bind(&stage2_transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Ingest sender connects to stage 1
    let ingest_sender = ZmqPipelineSender::new(&stage1_transport, PipelineConfig::default())
        .await
        .unwrap();
    // Compute sender connects to stage 2 (will forward processed data)
    let compute_sender = ZmqPipelineSender::new(&stage2_transport, PipelineConfig::default())
        .await
        .unwrap();
    tokio::time::sleep(SETTLE).await;

    // Push an ingest batch into stage 1
    let batch = IngestBatch {
        records: vec![Record {
            id: "rec-1".into(),
            fields: HashMap::from([
                ("user".into(), serde_json::json!("alice")),
                ("amount".into(), serde_json::json!(100.0)),
            ]),
        }],
    };
    ingest_sender
        .send(Message::new("pipeline.ingest", &batch).unwrap())
        .await
        .unwrap();

    // Stage 1 worker receives and "processes" it
    let stage1_msg = timeout(TIMEOUT, stage1_rx.recv())
        .await
        .expect("stage 1 timed out")
        .unwrap();
    let ingest_data: IngestBatch = stage1_msg.decode().unwrap();

    // Transform: extract features from the ingest data
    let features: Vec<Feature> = ingest_data
        .records
        .iter()
        .map(|r| Feature {
            name: "tx_amount".into(),
            entity_id: r.id.clone(),
            value: r
                .fields
                .get("amount")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0),
        })
        .collect();

    let compute_result = ComputeResult { features };
    compute_sender
        .send(Message::new("pipeline.compute", &compute_result).unwrap())
        .await
        .unwrap();

    // Stage 2 worker receives the computed result
    let stage2_msg = timeout(TIMEOUT, stage2_rx.recv())
        .await
        .expect("stage 2 timed out")
        .unwrap();
    let result: ComputeResult = stage2_msg.decode().unwrap();

    assert_eq!(result.features.len(), 1);
    assert_eq!(result.features[0].name, "tx_amount");
    assert_eq!(result.features[0].entity_id, "rec-1");
    assert!((result.features[0].value - 100.0).abs() < f64::EPSILON);
}

#[tokio::test]
async fn batch_send_preserves_order() {
    let transport = Transport::tcp("127.0.0.1", 16230);

    let receiver = ZmqPipelineReceiver::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let sender = ZmqPipelineSender::new(&transport, PipelineConfig::new(1000, 10))
        .await
        .unwrap();
    tokio::time::sleep(SETTLE).await;

    let messages: Vec<Message> = (0..10u32)
        .map(|i| Message::new("pipeline.ordered", &i).unwrap())
        .collect();

    let sent = sender.send_batch(&messages).await.unwrap();
    assert_eq!(sent, 10);

    // Receive all and verify order
    for expected in 0..10u32 {
        let msg = timeout(TIMEOUT, receiver.recv())
            .await
            .expect("timed out receiving batch item")
            .unwrap();
        let value: u32 = msg.decode().unwrap();
        assert_eq!(value, expected, "messages should arrive in order");
    }
}
