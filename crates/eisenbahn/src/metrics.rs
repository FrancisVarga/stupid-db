//! Metrics collection and HTTP exposure for the eisenbahn broker.
//!
//! Provides per-topic throughput tracking, worker health aggregation,
//! and a ring buffer of time-series snapshots exposed via `GET /metrics`.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Mutex;

use crate::messages::events::{WorkerHealth, WorkerStatus};

// ── Constants ────────────────────────────────────────────────────────

/// Ring buffer capacity: 5 minutes at 1-second granularity.
const RING_BUFFER_CAPACITY: usize = 300;

/// Snapshot interval for the time-series ring buffer.
const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(1);

// ── Per-topic stats ──────────────────────────────────────────────────

/// Accumulated stats for a single topic.
#[derive(Debug, Clone, Default)]
struct TopicStats {
    /// Total messages received on this topic.
    total_messages: u64,
    /// Total bytes received on this topic.
    total_bytes: u64,
    /// Messages received in the current 1-second window.
    window_messages: u64,
    /// Bytes received in the current 1-second window.
    window_bytes: u64,
    /// Messages/sec from the last completed window.
    messages_per_sec: f64,
    /// Bytes/sec from the last completed window.
    bytes_per_sec: f64,
}

/// JSON-serializable topic metrics for the HTTP response.
#[derive(Debug, Clone, Serialize)]
pub struct TopicMetrics {
    pub total_messages: u64,
    pub total_bytes: u64,
    pub messages_per_sec: f64,
    pub bytes_per_sec: f64,
}

// ── Worker health tracking ───────────────────────────────────────────

/// Tracked state for a single worker.
#[derive(Debug, Clone)]
struct WorkerState {
    status: WorkerStatus,
    cpu_pct: f64,
    mem_bytes: u64,
    last_seen: Instant,
}

/// JSON-serializable worker health for the HTTP response.
#[derive(Debug, Clone, Serialize)]
pub struct WorkerMetricsSnapshot {
    pub worker_id: String,
    pub status: String,
    pub cpu_pct: f64,
    pub mem_bytes: u64,
    pub last_seen_secs_ago: f64,
}

// ── Ring buffer ──────────────────────────────────────────────────────

/// A fixed-size ring buffer backed by `VecDeque`.
#[derive(Debug, Clone)]
pub struct RingBuffer<T> {
    buf: std::collections::VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// Create a new ring buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: std::collections::VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Push an item, evicting the oldest if at capacity.
    pub fn push(&mut self, item: T) {
        if self.buf.len() == self.capacity {
            self.buf.pop_front();
        }
        self.buf.push_back(item);
    }

    /// Number of items currently stored.
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    /// Iterate over items from oldest to newest.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.buf.iter()
    }
}

// ── Time-series snapshot ─────────────────────────────────────────────

/// A point-in-time snapshot stored in the ring buffer.
#[derive(Debug, Clone, Serialize)]
pub struct TimeSeriesPoint {
    /// Seconds since the collector was created.
    pub elapsed_secs: f64,
    /// Total messages across all topics at this point.
    pub total_messages: u64,
    /// Per-topic messages/sec at this point.
    pub topic_rates: HashMap<String, f64>,
}

// ── Full metrics response ────────────────────────────────────────────

/// Complete JSON response from `GET /metrics`.
#[derive(Debug, Clone, Serialize)]
pub struct MetricsResponse {
    pub topics: HashMap<String, TopicMetrics>,
    pub workers: Vec<WorkerMetricsSnapshot>,
    pub time_series: Vec<TimeSeriesPoint>,
    pub total_messages: u64,
    pub uptime_secs: f64,
}

// ── MetricsCollector ─────────────────────────────────────────────────

/// Inner mutable state protected by a mutex.
#[derive(Debug)]
struct Inner {
    topics: HashMap<String, TopicStats>,
    workers: HashMap<String, WorkerState>,
    ring: RingBuffer<TimeSeriesPoint>,
    total_messages: u64,
}

/// Thread-safe metrics collector for the broker.
///
/// Records per-topic message/byte counts, worker health pings, and
/// periodically snapshots rates into a ring buffer for time-series queries.
#[derive(Debug, Clone)]
pub struct MetricsCollector {
    inner: Arc<Mutex<Inner>>,
    start: Instant,
}

impl MetricsCollector {
    /// Create a new collector.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner {
                topics: HashMap::new(),
                workers: HashMap::new(),
                ring: RingBuffer::new(RING_BUFFER_CAPACITY),
                total_messages: 0,
            })),
            start: Instant::now(),
        }
    }

    /// Record a message on the given topic with the given byte size.
    pub async fn record_message(&self, topic: &str, byte_size: u64) {
        let mut inner = self.inner.lock().await;
        inner.total_messages += 1;
        let stats = inner.topics.entry(topic.to_string()).or_default();
        stats.total_messages += 1;
        stats.total_bytes += byte_size;
        stats.window_messages += 1;
        stats.window_bytes += byte_size;
    }

    /// Record a worker health ping.
    pub async fn record_worker_health(&self, health: &WorkerHealth) {
        let mut inner = self.inner.lock().await;
        inner.workers.insert(
            health.worker_id.clone(),
            WorkerState {
                status: health.status,
                cpu_pct: health.cpu_pct,
                mem_bytes: health.mem_bytes,
                last_seen: Instant::now(),
            },
        );
    }

    /// Flush the current 1-second window: roll rates and push a time-series point.
    ///
    /// Called by the background tick task every second.
    pub async fn tick(&self) {
        let mut inner = self.inner.lock().await;
        let elapsed = self.start.elapsed().as_secs_f64();

        let mut topic_rates = HashMap::new();
        for (name, stats) in &mut inner.topics {
            stats.messages_per_sec = stats.window_messages as f64;
            stats.bytes_per_sec = stats.window_bytes as f64;
            topic_rates.insert(name.clone(), stats.messages_per_sec);
            stats.window_messages = 0;
            stats.window_bytes = 0;
        }

        let point = TimeSeriesPoint {
            elapsed_secs: elapsed,
            total_messages: inner.total_messages,
            topic_rates,
        };
        inner.ring.push(point);
    }

    /// Build a complete snapshot for the HTTP response.
    pub async fn snapshot(&self) -> MetricsResponse {
        let inner = self.inner.lock().await;
        let now = Instant::now();

        let topics = inner
            .topics
            .iter()
            .map(|(name, stats)| {
                (
                    name.clone(),
                    TopicMetrics {
                        total_messages: stats.total_messages,
                        total_bytes: stats.total_bytes,
                        messages_per_sec: stats.messages_per_sec,
                        bytes_per_sec: stats.bytes_per_sec,
                    },
                )
            })
            .collect();

        let workers = inner
            .workers
            .iter()
            .map(|(id, state)| WorkerMetricsSnapshot {
                worker_id: id.clone(),
                status: format!("{:?}", state.status),
                cpu_pct: state.cpu_pct,
                mem_bytes: state.mem_bytes,
                last_seen_secs_ago: now.duration_since(state.last_seen).as_secs_f64(),
            })
            .collect();

        let time_series: Vec<TimeSeriesPoint> = inner.ring.iter().cloned().collect();

        MetricsResponse {
            topics,
            workers,
            time_series,
            total_messages: inner.total_messages,
            uptime_secs: self.start.elapsed().as_secs_f64(),
        }
    }
}

impl Default for MetricsCollector {
    fn default() -> Self {
        Self::new()
    }
}

// ── HTTP server ──────────────────────────────────────────────────────

/// Spawn the metrics HTTP server on the given port.
///
/// Returns a `JoinHandle` that resolves when the server shuts down.
pub fn spawn_metrics_server(
    port: u16,
    collector: MetricsCollector,
    shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let app = axum::Router::new()
            .route("/metrics", axum::routing::get(metrics_handler))
            .with_state(collector);

        let addr = std::net::SocketAddr::from(([0, 0, 0, 0], port));
        let listener = match tokio::net::TcpListener::bind(addr).await {
            Ok(l) => l,
            Err(e) => {
                tracing::error!(port, error = %e, "failed to bind metrics HTTP server");
                return;
            }
        };

        tracing::info!(port, "metrics HTTP server listening");

        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let mut rx = shutdown;
                while !*rx.borrow() {
                    if rx.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await
            .ok();

        tracing::info!("metrics HTTP server stopped");
    })
}

/// Spawn the background tick task that flushes rate windows every second.
pub fn spawn_tick_task(
    collector: MetricsCollector,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(SNAPSHOT_INTERVAL);
        loop {
            tokio::select! {
                _ = interval.tick() => {
                    collector.tick().await;
                }
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
            }
        }
    })
}

/// Axum handler: `GET /metrics` → JSON snapshot.
async fn metrics_handler(
    axum::extract::State(collector): axum::extract::State<MetricsCollector>,
) -> axum::Json<MetricsResponse> {
    axum::Json(collector.snapshot().await)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ring_buffer_respects_capacity() {
        let mut ring = RingBuffer::new(3);
        ring.push(1);
        ring.push(2);
        ring.push(3);
        assert_eq!(ring.len(), 3);

        ring.push(4);
        assert_eq!(ring.len(), 3);

        let items: Vec<_> = ring.iter().cloned().collect();
        assert_eq!(items, vec![2, 3, 4]);
    }

    #[test]
    fn ring_buffer_empty() {
        let ring: RingBuffer<i32> = RingBuffer::new(5);
        assert!(ring.is_empty());
        assert_eq!(ring.len(), 0);
    }

    #[test]
    fn ring_buffer_single_capacity() {
        let mut ring = RingBuffer::new(1);
        ring.push("a");
        ring.push("b");
        assert_eq!(ring.len(), 1);
        assert_eq!(ring.iter().next(), Some(&"b"));
    }

    #[tokio::test]
    async fn collector_records_messages() {
        let collector = MetricsCollector::new();
        collector.record_message("topic.a", 100).await;
        collector.record_message("topic.a", 200).await;
        collector.record_message("topic.b", 50).await;

        let snap = collector.snapshot().await;
        assert_eq!(snap.total_messages, 3);
        assert_eq!(snap.topics["topic.a"].total_messages, 2);
        assert_eq!(snap.topics["topic.a"].total_bytes, 300);
        assert_eq!(snap.topics["topic.b"].total_messages, 1);
    }

    #[tokio::test]
    async fn collector_tracks_worker_health() {
        let collector = MetricsCollector::new();
        collector
            .record_worker_health(&WorkerHealth {
                worker_id: "w1".into(),
                status: WorkerStatus::Healthy,
                cpu_pct: 42.0,
                mem_bytes: 1024,
            })
            .await;

        let snap = collector.snapshot().await;
        assert_eq!(snap.workers.len(), 1);
        assert_eq!(snap.workers[0].worker_id, "w1");
        assert_eq!(snap.workers[0].status, "Healthy");
        assert_eq!(snap.workers[0].cpu_pct, 42.0);
    }

    #[tokio::test]
    async fn tick_rolls_window_and_pushes_time_series() {
        let collector = MetricsCollector::new();
        collector.record_message("topic.a", 100).await;
        collector.record_message("topic.a", 200).await;

        collector.tick().await;

        let snap = collector.snapshot().await;
        // After tick, window counters are reset but totals remain.
        assert_eq!(snap.total_messages, 2);
        // One time-series point should exist.
        assert_eq!(snap.time_series.len(), 1);
        // The rate at that point should be 2 msg/s for topic.a.
        assert_eq!(snap.time_series[0].topic_rates["topic.a"], 2.0);
    }

    #[tokio::test]
    async fn tick_resets_window_counters() {
        let collector = MetricsCollector::new();
        collector.record_message("x", 10).await;
        collector.tick().await;

        // After tick, the rate should reflect the previous window.
        let snap = collector.snapshot().await;
        assert_eq!(snap.topics["x"].messages_per_sec, 1.0);

        // A second tick with no new messages → rate drops to 0.
        collector.tick().await;
        let snap = collector.snapshot().await;
        assert_eq!(snap.topics["x"].messages_per_sec, 0.0);
    }

    #[tokio::test]
    async fn worker_health_updates_last_seen() {
        let collector = MetricsCollector::new();
        collector
            .record_worker_health(&WorkerHealth {
                worker_id: "w1".into(),
                status: WorkerStatus::Degraded,
                cpu_pct: 90.0,
                mem_bytes: 2048,
            })
            .await;

        // Small sleep to ensure last_seen_secs_ago > 0.
        tokio::time::sleep(Duration::from_millis(10)).await;

        let snap = collector.snapshot().await;
        assert!(snap.workers[0].last_seen_secs_ago > 0.0);
        assert_eq!(snap.workers[0].status, "Degraded");
    }
}
