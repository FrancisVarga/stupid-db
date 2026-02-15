//! Micro-batch accumulator for queue messages.
//!
//! Collects [`QueueMessage`]s and flushes when either the size threshold
//! or time window is reached, whichever comes first. This balances
//! throughput (larger batches) with latency (time-bounded delivery).

use std::time::{Duration, Instant};

use crate::consumer::QueueMessage;

/// Accumulates queue messages into micro-batches.
///
/// Flushes when either the size threshold OR time window is reached,
/// whichever comes first.
pub struct MicroBatcher {
    buffer: Vec<QueueMessage>,
    max_size: usize,
    max_wait: Duration,
    batch_started: Option<Instant>,
}

impl MicroBatcher {
    /// Create a new batcher with the given size and time thresholds.
    ///
    /// - `max_size`: flush when this many messages are buffered.
    /// - `max_wait`: flush when this duration has elapsed since the first
    ///   message in the current batch was pushed.
    pub fn new(max_size: usize, max_wait: Duration) -> Self {
        Self {
            buffer: Vec::with_capacity(max_size),
            max_size,
            max_wait,
            batch_started: None,
        }
    }

    /// Add messages to the current batch.
    ///
    /// Starts the batch timer on the first non-empty push.
    pub fn push(&mut self, messages: Vec<QueueMessage>) {
        if self.batch_started.is_none() && !messages.is_empty() {
            self.batch_started = Some(Instant::now());
        }
        self.buffer.extend(messages);
    }

    /// Check if the batch should be flushed.
    ///
    /// Returns `true` when the buffer has reached `max_size` or
    /// `max_wait` has elapsed since the batch started.
    pub fn should_flush(&self) -> bool {
        if self.buffer.is_empty() {
            return false;
        }
        if self.buffer.len() >= self.max_size {
            return true;
        }
        if let Some(started) = self.batch_started {
            if started.elapsed() >= self.max_wait {
                return true;
            }
        }
        false
    }

    /// Flush the current batch, returning all accumulated messages.
    ///
    /// Resets the batcher for the next batch.
    pub fn flush(&mut self) -> Vec<QueueMessage> {
        self.batch_started = None;
        std::mem::take(&mut self.buffer)
    }

    /// Flush only if thresholds are met, otherwise return `None`.
    pub fn try_flush(&mut self) -> Option<Vec<QueueMessage>> {
        if self.should_flush() {
            Some(self.flush())
        } else {
            None
        }
    }

    /// Number of messages currently buffered.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Whether the buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_message(id: &str) -> QueueMessage {
        QueueMessage {
            id: id.to_string(),
            body: "{}".to_string(),
            receipt_handle: format!("handle-{id}"),
            timestamp: Utc::now(),
            attempt_count: 1,
        }
    }

    fn make_messages(count: usize) -> Vec<QueueMessage> {
        (0..count)
            .map(|i| make_message(&format!("msg-{i}")))
            .collect()
    }

    #[test]
    fn test_flush_on_size() {
        let mut batcher = MicroBatcher::new(3, Duration::from_secs(60));
        batcher.push(make_messages(3));
        assert!(batcher.should_flush());
    }

    #[test]
    fn test_no_flush_below_size() {
        let mut batcher = MicroBatcher::new(5, Duration::from_secs(60));
        batcher.push(make_messages(2));
        assert!(!batcher.should_flush());
    }

    #[test]
    fn test_flush_on_timeout() {
        let mut batcher = MicroBatcher::new(100, Duration::from_millis(10));
        batcher.push(make_messages(1));
        // Wait past the threshold
        std::thread::sleep(Duration::from_millis(20));
        assert!(batcher.should_flush());
    }

    #[test]
    fn test_try_flush_returns_none_when_not_ready() {
        let mut batcher = MicroBatcher::new(10, Duration::from_secs(60));
        batcher.push(make_messages(2));
        assert!(batcher.try_flush().is_none());
    }

    #[test]
    fn test_try_flush_returns_some_when_ready() {
        let mut batcher = MicroBatcher::new(2, Duration::from_secs(60));
        batcher.push(make_messages(2));
        let batch = batcher.try_flush();
        assert!(batch.is_some());
        assert_eq!(batch.unwrap().len(), 2);
    }

    #[test]
    fn test_flush_resets_state() {
        let mut batcher = MicroBatcher::new(2, Duration::from_secs(60));
        batcher.push(make_messages(3));
        let flushed = batcher.flush();
        assert_eq!(flushed.len(), 3);
        assert_eq!(batcher.len(), 0);
        assert!(batcher.is_empty());
        assert!(!batcher.should_flush());
    }

    #[test]
    fn test_empty_push_no_timer() {
        let mut batcher = MicroBatcher::new(5, Duration::from_millis(1));
        batcher.push(vec![]);
        // Timer should not have started, so even after waiting it shouldn't flush
        std::thread::sleep(Duration::from_millis(5));
        assert!(!batcher.should_flush());
    }

    #[test]
    fn test_multiple_pushes_accumulate() {
        let mut batcher = MicroBatcher::new(10, Duration::from_secs(60));
        batcher.push(make_messages(2));
        batcher.push(make_messages(3));
        assert_eq!(batcher.len(), 5);
        let flushed = batcher.flush();
        assert_eq!(flushed.len(), 5);
        // Verify IDs from both pushes are present
        assert_eq!(flushed[0].id, "msg-0");
        assert_eq!(flushed[2].id, "msg-0"); // Second push resets counter
    }
}
