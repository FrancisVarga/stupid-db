//! Worker trait and lifecycle management.
//!
//! Provides the [`Worker`] trait for defining long-running processes,
//! [`WorkerBuilder`] for fluent configuration, and [`WorkerRunner`] for
//! executing the event loop with automatic health pings and graceful shutdown.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Notify;
use tracing::{info, warn};

use crate::error::EisenbahnError;
use crate::message::Message;
use crate::messages::events::{WorkerHealth, WorkerStatus};
use crate::messages::topics::WORKER_HEALTH;
use crate::traits::EventPublisher;

// ── Worker trait ─────────────────────────────────────────────────────

/// A long-running process that participates in the eisenbahn messaging network.
///
/// Implementors define their startup/shutdown logic. The [`WorkerRunner`] handles
/// health pings, signal handling, and the event loop.
#[async_trait]
pub trait Worker: Send + Sync {
    /// Called once when the worker starts. Set up subscriptions, open connections, etc.
    async fn start(&self) -> Result<(), EisenbahnError>;

    /// Called once during graceful shutdown. Drain in-flight work, close connections.
    async fn stop(&self) -> Result<(), EisenbahnError>;

    /// Human-readable name for this worker (used in health pings and logging).
    fn name(&self) -> &str;
}

// ── Message handler type ─────────────────────────────────────────────

/// Boxed async function that handles an incoming message.
pub type MessageHandler =
    Box<dyn Fn(Message) -> Pin<Box<dyn Future<Output = Result<(), EisenbahnError>> + Send>> + Send + Sync>;

// ── WorkerBuilder ────────────────────────────────────────────────────

/// Fluent builder for configuring a [`WorkerRunner`].
///
/// # Example
/// ```ignore
/// let runner = WorkerBuilder::new("my-worker")
///     .health_interval(Duration::from_secs(10))
///     .on_message(|msg| Box::pin(async move {
///         println!("got: {}", msg.topic);
///         Ok(())
///     }))
///     .build();
/// ```
pub struct WorkerBuilder {
    name: String,
    health_interval: Duration,
    shutdown_timeout: Duration,
    message_handler: Option<MessageHandler>,
    subscriptions: Vec<String>,
}

impl WorkerBuilder {
    /// Create a new builder with the given worker name.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            health_interval: Duration::from_secs(30),
            shutdown_timeout: Duration::from_secs(5),
            message_handler: None,
            subscriptions: Vec::new(),
        }
    }

    /// Set the interval between health pings (default: 30s).
    pub fn health_interval(mut self, interval: Duration) -> Self {
        self.health_interval = interval;
        self
    }

    /// Set the maximum time to wait for in-flight work during shutdown (default: 5s).
    pub fn shutdown_timeout(mut self, timeout: Duration) -> Self {
        self.shutdown_timeout = timeout;
        self
    }

    /// Register a handler for incoming messages.
    pub fn on_message<F, Fut>(mut self, handler: F) -> Self
    where
        F: Fn(Message) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<(), EisenbahnError>> + Send + 'static,
    {
        self.message_handler = Some(Box::new(move |msg| Box::pin(handler(msg))));
        self
    }

    /// Add an event topic subscription.
    pub fn subscribe(mut self, topic: impl Into<String>) -> Self {
        self.subscriptions.push(topic.into());
        self
    }

    /// Build the runner configuration. The actual [`Worker`] impl and publisher
    /// are provided to [`WorkerRunner::run`].
    pub fn build(self) -> WorkerRunnerConfig {
        WorkerRunnerConfig {
            name: self.name,
            health_interval: self.health_interval,
            shutdown_timeout: self.shutdown_timeout,
            message_handler: self.message_handler,
            subscriptions: self.subscriptions,
        }
    }
}

// ── WorkerRunnerConfig ───────────────────────────────────────────────

/// Configuration produced by [`WorkerBuilder`], consumed by [`WorkerRunner`].
pub struct WorkerRunnerConfig {
    pub name: String,
    pub health_interval: Duration,
    pub shutdown_timeout: Duration,
    pub message_handler: Option<MessageHandler>,
    pub subscriptions: Vec<String>,
}

// ── WorkerRunner ─────────────────────────────────────────────────────

/// Runs a [`Worker`] with automatic health pings and graceful shutdown.
///
/// The runner manages three concurrent tasks:
/// 1. **Health ping loop** — publishes [`WorkerHealth`] at a configured interval
/// 2. **Signal handler** — listens for SIGINT/SIGTERM and initiates shutdown
/// 3. **Worker lifecycle** — calls `start()`, waits for shutdown, then calls `stop()`
pub struct WorkerRunner;

impl WorkerRunner {
    /// Run a worker to completion.
    ///
    /// This blocks until a shutdown signal is received or `shutdown_notify` is triggered.
    /// The `publisher` is used for health pings — it should be connected to the broker.
    pub async fn run(
        worker: Arc<dyn Worker>,
        publisher: Arc<dyn EventPublisher>,
        config: WorkerRunnerConfig,
        shutdown_notify: Option<Arc<Notify>>,
    ) -> Result<(), EisenbahnError> {
        let worker_name = config.name.clone();
        info!(worker = %worker_name, "starting worker");

        // Start the worker
        worker.start().await?;
        info!(worker = %worker_name, "worker started");

        // Publish initial health ping
        Self::publish_health(&*publisher, &worker_name, WorkerStatus::Healthy).await;

        // Create a shared shutdown signal
        let shutdown = Arc::new(Notify::new());

        // Spawn health ping loop
        let health_shutdown = shutdown.clone();
        let health_publisher = publisher.clone();
        let health_name = worker_name.clone();
        let health_interval = config.health_interval;
        let health_handle = tokio::spawn(async move {
            Self::health_loop(
                &*health_publisher,
                &health_name,
                health_interval,
                &health_shutdown,
            )
            .await;
        });

        // Wait for shutdown signal (OS signal or programmatic notify)
        let external_shutdown = shutdown_notify.clone();
        let sig_shutdown = shutdown.clone();
        let sig_name = worker_name.clone();
        let signal_handle = tokio::spawn(async move {
            Self::wait_for_shutdown(external_shutdown).await;
            info!(worker = %sig_name, "shutdown signal received");
            sig_shutdown.notify_waiters();
        });

        // Wait for shutdown to be triggered
        shutdown.notified().await;

        // Cancel health loop and signal handler
        health_handle.abort();
        signal_handle.abort();

        // Graceful shutdown: stop the worker with timeout
        info!(worker = %worker_name, timeout = ?config.shutdown_timeout, "stopping worker");
        match tokio::time::timeout(config.shutdown_timeout, worker.stop()).await {
            Ok(Ok(())) => {
                info!(worker = %worker_name, "worker stopped gracefully");
            }
            Ok(Err(e)) => {
                warn!(worker = %worker_name, error = %e, "worker stop returned error");
            }
            Err(_) => {
                warn!(worker = %worker_name, "worker stop timed out, forcing shutdown");
            }
        }

        // Final health ping: unhealthy (going down)
        Self::publish_health(&*publisher, &worker_name, WorkerStatus::Unhealthy).await;

        info!(worker = %worker_name, "worker shutdown complete");
        Ok(())
    }

    /// Periodically publish health pings until shutdown is signalled.
    async fn health_loop(
        publisher: &dyn EventPublisher,
        worker_name: &str,
        interval: Duration,
        shutdown: &Notify,
    ) {
        let mut ticker = tokio::time::interval(interval);
        // Skip the immediate first tick (we already sent an initial ping)
        ticker.tick().await;

        loop {
            tokio::select! {
                _ = ticker.tick() => {
                    Self::publish_health(publisher, worker_name, WorkerStatus::Healthy).await;
                }
                _ = shutdown.notified() => {
                    break;
                }
            }
        }
    }

    /// Publish a single health ping message.
    async fn publish_health(
        publisher: &dyn EventPublisher,
        worker_name: &str,
        status: WorkerStatus,
    ) {
        let health = WorkerHealth {
            worker_id: worker_name.to_string(),
            status,
            cpu_pct: 0.0,   // placeholder — real metrics can be added later
            mem_bytes: 0,
        };

        match Message::new(WORKER_HEALTH, &health) {
            Ok(msg) => {
                if let Err(e) = publisher.publish(msg).await {
                    warn!(worker = %worker_name, error = %e, "failed to publish health ping");
                }
            }
            Err(e) => {
                warn!(worker = %worker_name, error = %e, "failed to serialize health ping");
            }
        }
    }

    /// Wait for either an OS shutdown signal or a programmatic notification.
    async fn wait_for_shutdown(external: Option<Arc<Notify>>) {
        match external {
            Some(notify) => {
                tokio::select! {
                    _ = Self::os_signal() => {}
                    _ = notify.notified() => {}
                }
            }
            None => {
                Self::os_signal().await;
            }
        }
    }

    /// Wait for SIGINT or SIGTERM (Unix) or Ctrl+C (cross-platform fallback).
    async fn os_signal() {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT");
            let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
            tokio::select! {
                _ = sigint.recv() => {}
                _ = sigterm.recv() => {}
            }
        }

        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .expect("failed to listen for ctrl_c");
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use tokio::sync::Mutex;

    /// Mock publisher that records published messages.
    struct MockPublisher {
        messages: Mutex<Vec<Message>>,
    }

    impl MockPublisher {
        fn new() -> Self {
            Self {
                messages: Mutex::new(Vec::new()),
            }
        }

        async fn message_count(&self) -> usize {
            self.messages.lock().await.len()
        }

        async fn last_health(&self) -> Option<WorkerHealth> {
            let msgs = self.messages.lock().await;
            msgs.last().and_then(|m| m.decode::<WorkerHealth>().ok())
        }
    }

    #[async_trait]
    impl EventPublisher for MockPublisher {
        async fn publish(&self, message: Message) -> Result<(), EisenbahnError> {
            self.messages.lock().await.push(message);
            Ok(())
        }
    }

    /// Minimal worker for testing lifecycle.
    struct TestWorker {
        started: AtomicBool,
        stopped: AtomicBool,
        start_count: AtomicU32,
        stop_count: AtomicU32,
    }

    impl TestWorker {
        fn new() -> Self {
            Self {
                started: AtomicBool::new(false),
                stopped: AtomicBool::new(false),
                start_count: AtomicU32::new(0),
                stop_count: AtomicU32::new(0),
            }
        }
    }

    #[async_trait]
    impl Worker for TestWorker {
        async fn start(&self) -> Result<(), EisenbahnError> {
            self.started.store(true, Ordering::SeqCst);
            self.start_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        async fn stop(&self) -> Result<(), EisenbahnError> {
            self.stopped.store(true, Ordering::SeqCst);
            self.stop_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }

        fn name(&self) -> &str {
            "test-worker"
        }
    }

    #[tokio::test]
    async fn worker_lifecycle_start_health_stop() {
        let worker = Arc::new(TestWorker::new());
        let publisher = Arc::new(MockPublisher::new());
        let shutdown = Arc::new(Notify::new());

        let config = WorkerBuilder::new("test-worker")
            .health_interval(Duration::from_millis(50))
            .shutdown_timeout(Duration::from_secs(1))
            .build();

        // Run the worker in a background task
        let w = worker.clone();
        let p = publisher.clone();
        let s = shutdown.clone();
        let handle = tokio::spawn(async move {
            WorkerRunner::run(w, p, config, Some(s)).await
        });

        // Wait for at least one health ping cycle
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Verify worker started
        assert!(worker.started.load(Ordering::SeqCst), "worker should have started");

        // Verify health pings were published (initial + at least one periodic)
        let count = publisher.message_count().await;
        assert!(count >= 2, "expected ≥2 health pings, got {count}");

        // Trigger shutdown
        shutdown.notify_waiters();

        // Wait for completion
        let result = tokio::time::timeout(Duration::from_secs(5), handle)
            .await
            .expect("runner should complete within timeout")
            .expect("join handle should not panic");
        assert!(result.is_ok(), "runner should return Ok");

        // Verify worker stopped
        assert!(worker.stopped.load(Ordering::SeqCst), "worker should have stopped");

        // Verify exactly one start and one stop
        assert_eq!(worker.start_count.load(Ordering::SeqCst), 1);
        assert_eq!(worker.stop_count.load(Ordering::SeqCst), 1);

        // Verify final health ping is Unhealthy
        let last = publisher.last_health().await.expect("should have health messages");
        assert_eq!(last.status, WorkerStatus::Unhealthy);
        assert_eq!(last.worker_id, "test-worker");
    }

    #[tokio::test]
    async fn worker_builder_defaults() {
        let config = WorkerBuilder::new("default-worker").build();
        assert_eq!(config.name, "default-worker");
        assert_eq!(config.health_interval, Duration::from_secs(30));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(5));
        assert!(config.subscriptions.is_empty());
        assert!(config.message_handler.is_none());
    }

    #[tokio::test]
    async fn worker_builder_fluent_api() {
        let config = WorkerBuilder::new("custom")
            .health_interval(Duration::from_secs(10))
            .shutdown_timeout(Duration::from_secs(3))
            .subscribe("eisenbahn.ingest.complete")
            .subscribe("eisenbahn.anomaly.detected")
            .on_message(|_msg| async { Ok(()) })
            .build();

        assert_eq!(config.name, "custom");
        assert_eq!(config.health_interval, Duration::from_secs(10));
        assert_eq!(config.shutdown_timeout, Duration::from_secs(3));
        assert_eq!(config.subscriptions.len(), 2);
        assert!(config.message_handler.is_some());
    }

    #[tokio::test]
    async fn health_ping_contains_worker_id() {
        let publisher = Arc::new(MockPublisher::new());
        WorkerRunner::publish_health(&*publisher, "my-worker", WorkerStatus::Degraded).await;

        let health = publisher.last_health().await.expect("should have a message");
        assert_eq!(health.worker_id, "my-worker");
        assert_eq!(health.status, WorkerStatus::Degraded);
    }
}
