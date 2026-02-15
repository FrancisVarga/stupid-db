use std::collections::HashMap;
use std::time::Instant;

use chrono::{DateTime, Utc};
use serde::Serialize;

/// Pipeline performance metrics, updated incrementally by each stage.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineMetrics {
    // Stage 2 (hot path)
    /// Throughput of hot path document processing.
    pub hot_docs_per_second: f64,
    /// Average latency per document in microseconds.
    pub hot_avg_latency_us: f64,
    /// Average embedding generation time in milliseconds.
    pub hot_embedding_avg_ms: f64,

    // Stage 3 (warm compute)
    /// When warm compute last completed.
    pub warm_last_run: Option<DateTime<Utc>>,
    /// Duration of the last warm compute run in milliseconds.
    pub warm_duration_ms: u64,
    /// Number of events processed in the last warm run.
    pub warm_events_processed: u64,

    // Stage 4 (periodic)
    /// Last run time for each periodic task, keyed by task name.
    pub periodic_last_run: HashMap<String, DateTime<Utc>>,

    // Stage 5 (deep)
    /// When deep analysis last completed.
    pub deep_last_run: Option<DateTime<Utc>>,
    /// Duration of the last deep analysis in seconds.
    pub deep_duration_seconds: u64,

    // Internal counters (not serialized to API consumers).
    #[serde(skip)]
    hot_doc_count: u64,
    #[serde(skip)]
    hot_total_latency_us: f64,
    #[serde(skip)]
    hot_embedding_count: u64,
    #[serde(skip)]
    hot_total_embedding_ms: f64,
}

impl PipelineMetrics {
    /// Record a batch of documents processed on the hot path.
    ///
    /// Updates throughput and average latency metrics.
    pub fn record_hot_batch(&mut self, doc_count: u64, elapsed: std::time::Duration) {
        let elapsed_us = elapsed.as_micros() as f64;
        let elapsed_secs = elapsed.as_secs_f64();

        self.hot_doc_count += doc_count;
        self.hot_total_latency_us += elapsed_us;

        if elapsed_secs > 0.0 {
            self.hot_docs_per_second = doc_count as f64 / elapsed_secs;
        }

        if self.hot_doc_count > 0 {
            self.hot_avg_latency_us = self.hot_total_latency_us / self.hot_doc_count as f64;
        }
    }

    /// Record a single embedding generation duration.
    pub fn record_embedding(&mut self, elapsed: std::time::Duration) {
        self.hot_embedding_count += 1;
        self.hot_total_embedding_ms += elapsed.as_secs_f64() * 1000.0;

        if self.hot_embedding_count > 0 {
            self.hot_embedding_avg_ms =
                self.hot_total_embedding_ms / self.hot_embedding_count as f64;
        }
    }

    /// Record completion of a warm compute run.
    pub fn record_warm_run(&mut self, events_processed: u64, elapsed: std::time::Duration) {
        self.warm_last_run = Some(Utc::now());
        self.warm_duration_ms = elapsed.as_millis() as u64;
        self.warm_events_processed = events_processed;
    }

    /// Record completion of a periodic task.
    pub fn record_periodic_run(&mut self, task_name: &str) {
        self.periodic_last_run
            .insert(task_name.to_owned(), Utc::now());
    }

    /// Record completion of a deep analysis run.
    pub fn record_deep_run(&mut self, elapsed: std::time::Duration) {
        self.deep_last_run = Some(Utc::now());
        self.deep_duration_seconds = elapsed.as_secs();
    }

    /// Create a scoped timer that records hot batch metrics on drop.
    pub fn hot_timer(&self) -> HotTimer {
        HotTimer {
            start: Instant::now(),
        }
    }
}

/// A scoped timer for hot-path measurements.
pub struct HotTimer {
    start: Instant,
}

impl HotTimer {
    /// Finalize the timer and record metrics.
    pub fn finish(self, metrics: &mut PipelineMetrics, doc_count: u64) {
        metrics.record_hot_batch(doc_count, self.start.elapsed());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn hot_batch_metrics() {
        let mut m = PipelineMetrics::default();
        m.record_hot_batch(100, Duration::from_millis(50));

        assert!(m.hot_docs_per_second > 0.0);
        assert!(m.hot_avg_latency_us > 0.0);
    }

    #[test]
    fn warm_run_metrics() {
        let mut m = PipelineMetrics::default();
        m.record_warm_run(5000, Duration::from_millis(200));

        assert!(m.warm_last_run.is_some());
        assert_eq!(m.warm_events_processed, 5000);
        assert_eq!(m.warm_duration_ms, 200);
    }

    #[test]
    fn periodic_run_records_time() {
        let mut m = PipelineMetrics::default();
        m.record_periodic_run("pagerank");

        assert!(m.periodic_last_run.contains_key("pagerank"));
    }

    #[test]
    fn deep_run_metrics() {
        let mut m = PipelineMetrics::default();
        m.record_deep_run(Duration::from_secs(45));

        assert!(m.deep_last_run.is_some());
        assert_eq!(m.deep_duration_seconds, 45);
    }
}
