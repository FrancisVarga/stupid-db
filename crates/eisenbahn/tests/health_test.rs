//! Integration tests for worker health pings through the broker.
//!
//! Verifies that WorkerRunner publishes health pings via PUB/SUB,
//! and that the broker's MetricsCollector receives them.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;
use tokio::time::timeout;

use stupid_eisenbahn::broker::{BrokerConfig, EventBroker};
use stupid_eisenbahn::error::EisenbahnError;
use stupid_eisenbahn::messages::events::{WorkerHealth, WorkerStatus};
use stupid_eisenbahn::messages::topics;
use stupid_eisenbahn::EventSubscriber;
use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{Worker, WorkerBuilder, WorkerRunner, ZmqPublisher, ZmqSubscriber};

const TIMEOUT: Duration = Duration::from_secs(5);
const SETTLE: Duration = Duration::from_millis(200);

/// Minimal worker for testing.
struct NoopWorker {
    name: String,
}

impl NoopWorker {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

#[async_trait]
impl Worker for NoopWorker {
    async fn start(&self) -> Result<(), EisenbahnError> {
        Ok(())
    }
    async fn stop(&self) -> Result<(), EisenbahnError> {
        Ok(())
    }
    fn name(&self) -> &str {
        &self.name
    }
}

#[tokio::test]
async fn worker_health_pings_visible_to_subscriber() {
    // Start broker
    let cfg = BrokerConfig::tcp("127.0.0.1", 16400, 16401, 16402);
    let broker_handle = tokio::spawn(async move {
        let _ = EventBroker::new(cfg).run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16400);
    let backend = Transport::tcp("127.0.0.1", 16401);

    // Set up a subscriber to listen for health pings
    let health_sub = ZmqSubscriber::connect(&backend).await.unwrap();
    health_sub.subscribe(topics::WORKER_HEALTH).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Start a worker that publishes health pings through the broker
    let worker = Arc::new(NoopWorker::new("test-health-worker"));
    let publisher = Arc::new(ZmqPublisher::connect(&frontend).await.unwrap());
    let shutdown = Arc::new(Notify::new());

    let config = WorkerBuilder::new("test-health-worker")
        .health_interval(Duration::from_millis(100))
        .shutdown_timeout(Duration::from_secs(1))
        .build();

    let w = worker.clone();
    let p = publisher.clone();
    let s = shutdown.clone();
    let worker_handle = tokio::spawn(async move {
        WorkerRunner::run(w, p, config, Some(s)).await
    });
    tokio::time::sleep(SETTLE).await;

    // Should receive at least one health ping
    let msg = timeout(TIMEOUT, health_sub.recv())
        .await
        .expect("timed out waiting for health ping")
        .unwrap();

    assert_eq!(msg.topic, topics::WORKER_HEALTH);
    let health: WorkerHealth = msg.decode().unwrap();
    assert_eq!(health.worker_id, "test-health-worker");
    assert_eq!(health.status, WorkerStatus::Healthy);

    // Shut down the worker
    shutdown.notify_waiters();
    let _ = timeout(TIMEOUT, worker_handle).await;

    broker_handle.abort();
}

#[tokio::test]
async fn broker_collector_tracks_worker_health() {
    // Start broker and access its collector
    let cfg = BrokerConfig::tcp("127.0.0.1", 16410, 16411, 16412);
    let broker = EventBroker::new(cfg);
    let collector = broker.collector().clone();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16410);

    // Worker publishes health through the broker
    let worker = Arc::new(NoopWorker::new("tracked-worker"));
    let publisher = Arc::new(ZmqPublisher::connect(&frontend).await.unwrap());
    let shutdown = Arc::new(Notify::new());

    let config = WorkerBuilder::new("tracked-worker")
        .health_interval(Duration::from_millis(100))
        .shutdown_timeout(Duration::from_secs(1))
        .build();

    let w = worker.clone();
    let p = publisher.clone();
    let s = shutdown.clone();
    let worker_handle = tokio::spawn(async move {
        WorkerRunner::run(w, p, config, Some(s)).await
    });

    // Wait for a few health pings to arrive at the broker
    tokio::time::sleep(Duration::from_millis(500)).await;

    // Check the collector's snapshot
    let snapshot = collector.snapshot().await;

    // The broker should have seen worker health messages
    assert!(
        snapshot.total_messages > 0,
        "broker should have processed health messages"
    );
    assert!(
        snapshot.topics.contains_key(topics::WORKER_HEALTH),
        "broker should track worker health topic"
    );

    // Worker health should appear in the collector
    let worker_snap = snapshot
        .workers
        .iter()
        .find(|w| w.worker_id == "tracked-worker");
    assert!(
        worker_snap.is_some(),
        "collector should track the worker: workers = {:?}",
        snapshot.workers
    );

    if let Some(w) = worker_snap {
        assert_eq!(w.status, "Healthy");
    }

    shutdown.notify_waiters();
    let _ = timeout(TIMEOUT, worker_handle).await;
    broker_handle.abort();
}

#[tokio::test]
async fn multiple_workers_health_pings() {
    let cfg = BrokerConfig::tcp("127.0.0.1", 16420, 16421, 16422);
    let broker = EventBroker::new(cfg);
    let collector = broker.collector().clone();

    let broker_handle = tokio::spawn(async move {
        let _ = broker.run().await;
    });
    tokio::time::sleep(SETTLE).await;

    let frontend = Transport::tcp("127.0.0.1", 16420);

    let shutdown1 = Arc::new(Notify::new());
    let shutdown2 = Arc::new(Notify::new());

    // Start two workers
    let w1 = Arc::new(NoopWorker::new("worker-alpha"));
    let p1 = Arc::new(ZmqPublisher::connect(&frontend).await.unwrap());
    let cfg1 = WorkerBuilder::new("worker-alpha")
        .health_interval(Duration::from_millis(100))
        .build();
    let h1 = {
        let w = w1.clone();
        let p = p1.clone();
        let s = shutdown1.clone();
        tokio::spawn(async move { WorkerRunner::run(w, p, cfg1, Some(s)).await })
    };

    let w2 = Arc::new(NoopWorker::new("worker-beta"));
    let p2 = Arc::new(ZmqPublisher::connect(&frontend).await.unwrap());
    let cfg2 = WorkerBuilder::new("worker-beta")
        .health_interval(Duration::from_millis(100))
        .build();
    let h2 = {
        let w = w2.clone();
        let p = p2.clone();
        let s = shutdown2.clone();
        tokio::spawn(async move { WorkerRunner::run(w, p, cfg2, Some(s)).await })
    };

    // Wait for health pings
    tokio::time::sleep(Duration::from_millis(500)).await;

    let snapshot = collector.snapshot().await;
    let worker_ids: Vec<&str> = snapshot.workers.iter().map(|w| w.worker_id.as_str()).collect();

    assert!(
        worker_ids.contains(&"worker-alpha"),
        "should track worker-alpha: {:?}",
        worker_ids
    );
    assert!(
        worker_ids.contains(&"worker-beta"),
        "should track worker-beta: {:?}",
        worker_ids
    );

    shutdown1.notify_waiters();
    shutdown2.notify_waiters();
    let _ = timeout(TIMEOUT, h1).await;
    let _ = timeout(TIMEOUT, h2).await;
    broker_handle.abort();
}
