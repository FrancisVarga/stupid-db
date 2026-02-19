//! Integration tests for the EventBroker.
//!
//! These tests verify that the broker correctly proxies messages from
//! publishers (frontend/SUB) to subscribers (backend/PUB) and that
//! the health check REP socket responds to pings.

use std::time::Duration;

use tokio::time::timeout;
use zeromq::prelude::*;
use zeromq::{ReqSocket, ZmqMessage};

use stupid_eisenbahn::broker::{BrokerConfig, EventBroker};
use stupid_eisenbahn::messages::events::IngestComplete;
use stupid_eisenbahn::messages::topics;
use stupid_eisenbahn::{EventPublisher, EventSubscriber};
use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{Message, ZmqPublisher, ZmqSubscriber};

const TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(200);

/// Helper: start broker in background, return its handle.
async fn start_broker(config: BrokerConfig) -> tokio::task::JoinHandle<()> {
    let broker = EventBroker::new(config);
    tokio::spawn(async move {
        let _ = broker.run().await;
    })
}

#[tokio::test]
async fn broker_proxies_single_message() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16000, 16001, 16002);
    let handle = start_broker(cfg).await;
    tokio::time::sleep(SETTLE).await;

    let publisher = ZmqPublisher::connect(&Transport::tcp("127.0.0.1", 16000))
        .await
        .unwrap();
    let subscriber = ZmqSubscriber::connect(&Transport::tcp("127.0.0.1", 16001))
        .await
        .unwrap();
    subscriber.subscribe("eisenbahn.").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let event = IngestComplete {
        source: "test.parquet".into(),
        record_count: 500,
        duration_ms: 10,
        job_id: None,
        total_segments: 0,
        error: None,
        source_type: None,
    };
    let msg = Message::new(topics::INGEST_COMPLETE, &event).unwrap();
    let cid = msg.correlation_id;
    publisher.publish(msg).await.unwrap();

    let received = timeout(TIMEOUT, subscriber.recv())
        .await
        .expect("timed out")
        .unwrap();
    assert_eq!(received.topic, topics::INGEST_COMPLETE);
    assert_eq!(received.correlation_id, cid);

    let decoded: IngestComplete = received.decode().unwrap();
    assert_eq!(decoded.source, "test.parquet");
    assert_eq!(decoded.record_count, 500);

    handle.abort();
}

#[tokio::test]
async fn broker_proxies_multiple_topics() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16010, 16011, 16012);
    let handle = start_broker(cfg).await;
    tokio::time::sleep(SETTLE).await;

    let publisher = ZmqPublisher::connect(&Transport::tcp("127.0.0.1", 16010))
        .await
        .unwrap();

    // Two subscribers: one for ingest, one for anomaly
    let sub_ingest = ZmqSubscriber::connect(&Transport::tcp("127.0.0.1", 16011))
        .await
        .unwrap();
    sub_ingest.subscribe("eisenbahn.ingest").await.unwrap();

    let sub_anomaly = ZmqSubscriber::connect(&Transport::tcp("127.0.0.1", 16011))
        .await
        .unwrap();
    sub_anomaly.subscribe("eisenbahn.anomaly").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Publish an ingest event
    let ingest_event = IngestComplete {
        source: "a.parquet".into(),
        record_count: 100,
        duration_ms: 5,
        job_id: None,
        total_segments: 0,
        error: None,
        source_type: None,
    };
    publisher
        .publish(Message::new(topics::INGEST_COMPLETE, &ingest_event).unwrap())
        .await
        .unwrap();

    // Publish an anomaly event
    use stupid_eisenbahn::messages::events::AnomalyDetected;
    let anomaly_event = AnomalyDetected {
        rule_id: "rule-1".into(),
        entity_id: "e-1".into(),
        score: 0.99,
    };
    publisher
        .publish(Message::new(topics::ANOMALY_DETECTED, &anomaly_event).unwrap())
        .await
        .unwrap();

    // Ingest subscriber should get only the ingest event
    let r1 = timeout(TIMEOUT, sub_ingest.recv())
        .await
        .expect("ingest sub timed out")
        .unwrap();
    assert_eq!(r1.topic, topics::INGEST_COMPLETE);

    // Anomaly subscriber should get only the anomaly event
    let r2 = timeout(TIMEOUT, sub_anomaly.recv())
        .await
        .expect("anomaly sub timed out")
        .unwrap();
    assert_eq!(r2.topic, topics::ANOMALY_DETECTED);

    // Neither should receive the other's event
    let extra_ingest = timeout(Duration::from_millis(300), sub_ingest.recv()).await;
    assert!(extra_ingest.is_err(), "ingest sub should not get anomaly event");

    let extra_anomaly = timeout(Duration::from_millis(300), sub_anomaly.recv()).await;
    assert!(extra_anomaly.is_err(), "anomaly sub should not get ingest event");

    handle.abort();
}

#[tokio::test]
async fn broker_health_check_responds_ok() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16020, 16021, 16022);
    let handle = start_broker(cfg).await;
    tokio::time::sleep(SETTLE).await;

    // Connect a REQ socket to the health endpoint
    let mut req = ReqSocket::new();
    req.connect("tcp://127.0.0.1:16022").await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send a ping
    let ping: ZmqMessage = "ping".into();
    req.send(ping).await.unwrap();

    // Should get "ok" back
    let reply = timeout(TIMEOUT, req.recv())
        .await
        .expect("health check timed out")
        .unwrap();

    let reply_str = reply
        .iter()
        .next()
        .map(|f| String::from_utf8_lossy(f.as_ref()).to_string())
        .unwrap_or_default();
    assert_eq!(reply_str, "ok");

    handle.abort();
}

#[tokio::test]
async fn broker_metrics_count_forwarded_messages() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16030, 16031, 16032);
    let broker = EventBroker::new(cfg);
    let metrics = broker.metrics().clone();

    let handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let publisher = ZmqPublisher::connect(&Transport::tcp("127.0.0.1", 16030))
        .await
        .unwrap();
    let subscriber = ZmqSubscriber::connect(&Transport::tcp("127.0.0.1", 16031))
        .await
        .unwrap();
    subscriber.subscribe("").await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Send 3 messages
    for i in 0..3u32 {
        let msg = Message::new("eisenbahn.test.count", &i).unwrap();
        publisher.publish(msg).await.unwrap();
    }

    // Receive all 3
    for _ in 0..3 {
        timeout(TIMEOUT, subscriber.recv())
            .await
            .expect("timed out")
            .unwrap();
    }

    // Allow broker loop to update metrics
    tokio::time::sleep(Duration::from_millis(100)).await;

    assert_eq!(metrics.total(), 3, "broker should count 3 forwarded messages");

    handle.abort();
}
