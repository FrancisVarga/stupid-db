//! Integration tests for PUB/SUB through the broker.
//!
//! These tests verify multi-publisher, multi-subscriber scenarios
//! with topic filtering all going through the central EventBroker.

use std::time::Duration;

use tokio::time::timeout;

use stupid_eisenbahn::broker::{BrokerConfig, EventBroker};
use stupid_eisenbahn::messages::events::{
    AnomalyDetected, ComputeComplete, IngestComplete, RuleAction, RuleChanged,
};
use stupid_eisenbahn::messages::topics;
use stupid_eisenbahn::{EventPublisher, EventSubscriber};
use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{Message, ZmqPublisher, ZmqSubscriber};

const TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(200);

#[tokio::test]
async fn multiple_publishers_single_subscriber() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16100, 16101, 16102);
    let handle = tokio::spawn(async move {
        let _ = EventBroker::new(cfg).run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16100);
    let backend = Transport::tcp("127.0.0.1", 16101);

    // Two publishers
    let pub1 = ZmqPublisher::connect(&frontend).await.unwrap();
    let pub2 = ZmqPublisher::connect(&frontend).await.unwrap();

    // One subscriber for all eisenbahn events
    let sub = ZmqSubscriber::connect(&backend).await.unwrap();
    sub.subscribe("eisenbahn.").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // pub1 sends ingest event
    let ingest = IngestComplete {
        source: "pub1.parquet".into(),
        record_count: 200,
        duration_ms: 20,
        job_id: None,
        total_segments: 0,
        error: None,
        source_type: None,
    };
    pub1.publish(Message::new(topics::INGEST_COMPLETE, &ingest).unwrap())
        .await
        .unwrap();

    // pub2 sends anomaly event
    let anomaly = AnomalyDetected {
        rule_id: "r-1".into(),
        entity_id: "e-1".into(),
        score: 0.75,
    };
    pub2.publish(Message::new(topics::ANOMALY_DETECTED, &anomaly).unwrap())
        .await
        .unwrap();

    // Collect both messages (order may vary)
    let mut received_topics = Vec::new();
    for _ in 0..2 {
        let msg = timeout(TIMEOUT, sub.recv())
            .await
            .expect("timed out")
            .unwrap();
        received_topics.push(msg.topic.clone());
    }

    received_topics.sort();
    assert!(received_topics.contains(&topics::INGEST_COMPLETE.to_string()));
    assert!(received_topics.contains(&topics::ANOMALY_DETECTED.to_string()));

    handle.abort();
}

#[tokio::test]
async fn fan_out_to_multiple_subscribers() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16110, 16111, 16112);
    let handle = tokio::spawn(async move {
        let _ = EventBroker::new(cfg).run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16110);
    let backend = Transport::tcp("127.0.0.1", 16111);

    let publisher = ZmqPublisher::connect(&frontend).await.unwrap();

    // Two subscribers both subscribing to the same topic
    let sub1 = ZmqSubscriber::connect(&backend).await.unwrap();
    let sub2 = ZmqSubscriber::connect(&backend).await.unwrap();
    sub1.subscribe("eisenbahn.compute").await.unwrap();
    sub2.subscribe("eisenbahn.compute").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let event = ComputeComplete {
        batch_id: "batch-1".into(),
        features_computed: 64,
    };
    publisher
        .publish(Message::new(topics::COMPUTE_COMPLETE, &event).unwrap())
        .await
        .unwrap();

    // Both subscribers should receive the message (fan-out)
    let r1 = timeout(TIMEOUT, sub1.recv())
        .await
        .expect("sub1 timed out")
        .unwrap();
    let r2 = timeout(TIMEOUT, sub2.recv())
        .await
        .expect("sub2 timed out")
        .unwrap();

    assert_eq!(r1.topic, topics::COMPUTE_COMPLETE);
    assert_eq!(r2.topic, topics::COMPUTE_COMPLETE);

    let d1: ComputeComplete = r1.decode().unwrap();
    let d2: ComputeComplete = r2.decode().unwrap();
    assert_eq!(d1.batch_id, "batch-1");
    assert_eq!(d2.features_computed, 64);

    handle.abort();
}

#[tokio::test]
async fn all_event_types_through_broker() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16120, 16121, 16122);
    let handle = tokio::spawn(async move {
        let _ = EventBroker::new(cfg).run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16120);
    let backend = Transport::tcp("127.0.0.1", 16121);

    let publisher = ZmqPublisher::connect(&frontend).await.unwrap();
    let subscriber = ZmqSubscriber::connect(&backend).await.unwrap();
    subscriber.subscribe("eisenbahn.").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Send all event types
    let events: Vec<(&str, Message)> = vec![
        (
            "ingest",
            Message::new(
                topics::INGEST_COMPLETE,
                &IngestComplete {
                    source: "s".into(),
                    record_count: 1,
                    duration_ms: 1,
                    job_id: None,
                    total_segments: 0,
                    error: None,
                    source_type: None,
                },
            )
            .unwrap(),
        ),
        (
            "anomaly",
            Message::new(
                topics::ANOMALY_DETECTED,
                &AnomalyDetected {
                    rule_id: "r".into(),
                    entity_id: "e".into(),
                    score: 0.5,
                },
            )
            .unwrap(),
        ),
        (
            "rule",
            Message::new(
                topics::RULE_CHANGED,
                &RuleChanged {
                    rule_id: "r".into(),
                    action: RuleAction::Created,
                },
            )
            .unwrap(),
        ),
        (
            "compute",
            Message::new(
                topics::COMPUTE_COMPLETE,
                &ComputeComplete {
                    batch_id: "b".into(),
                    features_computed: 10,
                },
            )
            .unwrap(),
        ),
    ];

    for (_, msg) in &events {
        publisher.publish(msg.clone()).await.unwrap();
    }

    // Receive all 4 events
    let mut received_topics = Vec::new();
    for _ in 0..4 {
        let msg = timeout(TIMEOUT, subscriber.recv())
            .await
            .expect("timed out receiving event")
            .unwrap();
        received_topics.push(msg.topic.clone());
    }

    assert_eq!(received_topics.len(), 4);
    received_topics.sort();
    let mut expected = vec![
        topics::INGEST_COMPLETE,
        topics::ANOMALY_DETECTED,
        topics::RULE_CHANGED,
        topics::COMPUTE_COMPLETE,
    ];
    expected.sort();
    assert_eq!(received_topics, expected);

    handle.abort();
}
