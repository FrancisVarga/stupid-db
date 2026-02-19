//! Full pipeline roundtrip integration test.
//!
//! Simulates the complete data flow: ingest → compute → graph
//! using both PUB/SUB events (notifications) and PUSH/PULL pipelines (data).

use std::collections::HashMap;
use std::time::Duration;

use tokio::time::timeout;

use stupid_eisenbahn::broker::{BrokerConfig, EventBroker};
use stupid_eisenbahn::messages::events::{ComputeComplete, IngestComplete};
use stupid_eisenbahn::messages::pipeline::{
    ComputeResult, Edge, Entity, Feature, GraphUpdate, IngestBatch, Record,
};
use stupid_eisenbahn::messages::topics;
use stupid_eisenbahn::{EventPublisher, EventSubscriber, PipelineReceiver, PipelineSender};
use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{
    Message, PipelineConfig, ZmqPipelineReceiver, ZmqPipelineSender, ZmqPublisher, ZmqSubscriber,
};

const TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(200);

/// Full pipeline roundtrip: ingest → compute → graph with PUB/SUB notifications.
///
/// Architecture:
/// ```text
///   [Producer] --PUSH--> [Ingest Worker] --PUSH--> [Compute Worker] --PUSH--> [Graph Worker]
///                              |                         |                         |
///                              |--- PUB(ingest.complete) |--- PUB(compute.complete)|
///                              |                         |                         |
///                         [Event Broker] ─── SUB ─── [Dashboard Subscriber]
/// ```
#[tokio::test]
async fn full_ingest_compute_graph_roundtrip() {
    // ── Step 1: Start the event broker ──────────────────────────────
    let broker_cfg = BrokerConfig::tcp("127.0.0.1", 16300, 16301, 16302);
    let broker_handle = tokio::spawn(async move {
        let _ = EventBroker::new(broker_cfg).run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let broker_frontend = Transport::tcp("127.0.0.1", 16300);
    let broker_backend = Transport::tcp("127.0.0.1", 16301);

    // ── Step 2: Set up pipeline stages ──────────────────────────────
    let ingest_transport = Transport::tcp("127.0.0.1", 16310);
    let compute_transport = Transport::tcp("127.0.0.1", 16311);
    let graph_transport = Transport::tcp("127.0.0.1", 16312);

    // Pipeline receivers bind first (stable endpoints)
    let ingest_rx = ZmqPipelineReceiver::bind(&ingest_transport).await.unwrap();
    let compute_rx = ZmqPipelineReceiver::bind(&compute_transport).await.unwrap();
    let graph_rx = ZmqPipelineReceiver::bind(&graph_transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // ── Step 3: Set up PUB/SUB for event notifications ──────────────
    let event_publisher = ZmqPublisher::connect(&broker_frontend).await.unwrap();
    let dashboard_sub = ZmqSubscriber::connect(&broker_backend).await.unwrap();
    dashboard_sub.subscribe("eisenbahn.").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // ── Step 4: Producer pushes raw data into the ingest pipeline ───
    let producer_tx = ZmqPipelineSender::new(&ingest_transport, PipelineConfig::default())
        .await
        .unwrap();

    let raw_batch = IngestBatch {
        records: vec![
            Record {
                id: "tx-001".into(),
                fields: HashMap::from([
                    ("user".into(), serde_json::json!("alice")),
                    ("amount".into(), serde_json::json!(250.0)),
                    ("ip".into(), serde_json::json!("10.0.0.1")),
                ]),
            },
            Record {
                id: "tx-002".into(),
                fields: HashMap::from([
                    ("user".into(), serde_json::json!("bob")),
                    ("amount".into(), serde_json::json!(75.0)),
                    ("ip".into(), serde_json::json!("10.0.0.2")),
                ]),
            },
        ],
    };

    producer_tx
        .send(Message::new(topics::INGEST_BATCH, &raw_batch).unwrap())
        .await
        .unwrap();

    // ── Step 5: Ingest worker receives, processes, forwards ─────────
    let ingest_msg = timeout(TIMEOUT, ingest_rx.recv())
        .await
        .expect("ingest rx timed out")
        .unwrap();
    let batch: IngestBatch = ingest_msg.decode().unwrap();
    assert_eq!(batch.records.len(), 2);

    // Ingest worker extracts features and pushes to compute stage
    let compute_tx = ZmqPipelineSender::new(&compute_transport, PipelineConfig::default())
        .await
        .unwrap();

    let features: Vec<Feature> = batch
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

    compute_tx
        .send(Message::new(topics::COMPUTE_RESULT, &ComputeResult { features }).unwrap())
        .await
        .unwrap();

    // Ingest worker publishes completion event
    event_publisher
        .publish(
            Message::new(
                topics::INGEST_COMPLETE,
                &IngestComplete {
                    source: "raw_batch".into(),
                    record_count: batch.records.len() as u64,
                    duration_ms: 15,
                    job_id: None,
                    total_segments: 0,
                    error: None,
                    source_type: None,
                },
            )
            .unwrap(),
        )
        .await
        .unwrap();

    // ── Step 6: Compute worker receives, processes, forwards ────────
    let compute_msg = timeout(TIMEOUT, compute_rx.recv())
        .await
        .expect("compute rx timed out")
        .unwrap();
    let compute_result: ComputeResult = compute_msg.decode().unwrap();
    assert_eq!(compute_result.features.len(), 2);

    // Compute worker builds graph updates from features
    let graph_tx = ZmqPipelineSender::new(&graph_transport, PipelineConfig::default())
        .await
        .unwrap();

    let entities: Vec<Entity> = batch
        .records
        .iter()
        .filter_map(|r| {
            r.fields.get("user").and_then(|v| v.as_str()).map(|user| Entity {
                id: user.to_string(),
                entity_type: "user".into(),
                properties: HashMap::from([("source".into(), r.id.clone())]),
            })
        })
        .collect();

    let edges: Vec<Edge> = batch
        .records
        .iter()
        .filter_map(|r| {
            let user = r.fields.get("user")?.as_str()?;
            let ip = r.fields.get("ip")?.as_str()?;
            Some(Edge {
                source_id: user.to_string(),
                target_id: ip.to_string(),
                edge_type: "connected_from".into(),
                weight: 1.0,
            })
        })
        .collect();

    graph_tx
        .send(Message::new(topics::GRAPH_UPDATE, &GraphUpdate { entities, edges }).unwrap())
        .await
        .unwrap();

    // Compute worker publishes completion event
    event_publisher
        .publish(
            Message::new(
                topics::COMPUTE_COMPLETE,
                &ComputeComplete {
                    batch_id: "compute-001".into(),
                    features_computed: compute_result.features.len() as u64,
                },
            )
            .unwrap(),
        )
        .await
        .unwrap();

    // ── Step 7: Graph worker receives the update ────────────────────
    let graph_msg = timeout(TIMEOUT, graph_rx.recv())
        .await
        .expect("graph rx timed out")
        .unwrap();
    let graph_update: GraphUpdate = graph_msg.decode().unwrap();
    assert_eq!(graph_update.entities.len(), 2);
    assert_eq!(graph_update.edges.len(), 2);
    assert_eq!(graph_update.entities[0].entity_type, "user");
    assert_eq!(graph_update.edges[0].edge_type, "connected_from");

    // ── Step 8: Dashboard receives event notifications ──────────────
    let mut dashboard_events = Vec::new();
    for _ in 0..2 {
        let msg = timeout(TIMEOUT, dashboard_sub.recv())
            .await
            .expect("dashboard timed out");
        if let Ok(msg) = msg {
            dashboard_events.push(msg.topic.clone());
        }
    }

    dashboard_events.sort();
    assert!(
        dashboard_events.contains(&topics::INGEST_COMPLETE.to_string()),
        "dashboard should see ingest completion"
    );
    assert!(
        dashboard_events.contains(&topics::COMPUTE_COMPLETE.to_string()),
        "dashboard should see compute completion"
    );

    broker_handle.abort();
}

/// Test correlation ID propagation across pipeline stages.
#[tokio::test]
async fn correlation_id_propagation() {
    let transport1 = Transport::tcp("127.0.0.1", 16320);
    let transport2 = Transport::tcp("127.0.0.1", 16321);

    let rx1 = ZmqPipelineReceiver::bind(&transport1).await.unwrap();
    let rx2 = ZmqPipelineReceiver::bind(&transport2).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let tx1 = ZmqPipelineSender::new(&transport1, PipelineConfig::default())
        .await
        .unwrap();
    let tx2 = ZmqPipelineSender::new(&transport2, PipelineConfig::default())
        .await
        .unwrap();
    tokio::time::sleep(SETTLE).await;

    // Send initial message
    let original = Message::new("stage1", &"initial payload".to_string()).unwrap();
    let original_cid = original.correlation_id;
    tx1.send(original).await.unwrap();

    // Stage 1 receives and forwards with same correlation ID
    let stage1_msg = timeout(TIMEOUT, rx1.recv())
        .await
        .expect("stage1 timed out")
        .unwrap();
    assert_eq!(stage1_msg.correlation_id, original_cid);

    // Create continuation message preserving correlation
    let continuation =
        Message::with_correlation("stage2", &"processed".to_string(), stage1_msg.correlation_id)
            .unwrap();
    tx2.send(continuation).await.unwrap();

    // Stage 2 receives with preserved correlation
    let stage2_msg = timeout(TIMEOUT, rx2.recv())
        .await
        .expect("stage2 timed out")
        .unwrap();
    assert_eq!(
        stage2_msg.correlation_id, original_cid,
        "correlation ID should be preserved across pipeline stages"
    );
}
