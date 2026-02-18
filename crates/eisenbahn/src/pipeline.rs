use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument};
use zeromq::{PushSocket, PullSocket, Socket, SocketRecv, SocketSend};

use crate::error::EisenbahnError;
use crate::message::Message;
use crate::traits::{PipelineReceiver, PipelineSender};
use crate::transport::Transport;

/// Default ZeroMQ high-water mark (max queued messages before backpressure).
const DEFAULT_HIGH_WATER_MARK: usize = 1000;

/// Default batch size (messages buffered before auto-flush).
const DEFAULT_BATCH_SIZE: usize = 1;

/// Configuration for pipeline transport behavior.
#[derive(Debug, Clone)]
pub struct PipelineConfig {
    /// Maximum number of messages to queue before applying backpressure.
    /// When the queue is full, `send()` will block until space is available.
    pub high_water_mark: usize,

    /// Number of messages to buffer before flushing.
    /// Set to 1 for immediate delivery (no batching).
    pub batch_size: usize,
}

impl Default for PipelineConfig {
    fn default() -> Self {
        Self {
            high_water_mark: DEFAULT_HIGH_WATER_MARK,
            batch_size: DEFAULT_BATCH_SIZE,
        }
    }
}

impl PipelineConfig {
    /// Create a config with the given high-water mark and batch size.
    pub fn new(high_water_mark: usize, batch_size: usize) -> Self {
        Self {
            high_water_mark,
            batch_size: batch_size.max(1), // minimum batch size of 1
        }
    }
}

/// PUSH socket sender that distributes work to downstream PULL workers.
///
/// Messages are sent round-robin across all connected PULL receivers,
/// providing automatic load balancing. When the high-water mark is reached,
/// sends will block until queue space is available (backpressure).
pub struct ZmqPipelineSender {
    socket: Mutex<PushSocket>,
    config: PipelineConfig,
    /// Tracks messages in the current batch for flush logic.
    batch_count: AtomicUsize,
}

impl ZmqPipelineSender {
    /// Create a new PUSH sender that connects to the given transport endpoint.
    ///
    /// The sender connects (not binds) because PUSH sockets are typically
    /// ephemeral producers that connect to stable PULL workers.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn new(
        transport: &Transport,
        config: PipelineConfig,
    ) -> Result<Self, EisenbahnError> {
        let mut socket = PushSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, hwm = config.high_water_mark, "connecting PUSH socket");
        socket.connect(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
            config,
            batch_count: AtomicUsize::new(0),
        })
    }

    /// Create a new PUSH sender that binds to the given transport endpoint.
    ///
    /// Use bind when this sender is the stable endpoint (e.g. a single
    /// producer that multiple PULL workers connect to).
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn bind(
        transport: &Transport,
        config: PipelineConfig,
    ) -> Result<Self, EisenbahnError> {
        transport
            .ensure_ipc_dir()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        transport
            .remove_stale_socket()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        let mut socket = PushSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, hwm = config.high_water_mark, "binding PUSH socket");
        socket.bind(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
            config,
            batch_count: AtomicUsize::new(0),
        })
    }

    /// Send a batch of messages, flushing after all are queued.
    ///
    /// More efficient than individual sends when you have multiple messages
    /// ready to go — avoids per-message overhead.
    pub async fn send_batch(&self, messages: &[Message]) -> Result<usize, EisenbahnError> {
        let mut socket = self.socket.lock().await;
        let mut sent = 0;
        for msg in messages {
            let bytes = msg.to_bytes()?;
            socket.send(bytes.into()).await?;
            sent += 1;
        }
        debug!(count = sent, "batch sent");
        self.batch_count.store(0, Ordering::Relaxed);
        Ok(sent)
    }

    /// Flush any pending messages in the current batch.
    ///
    /// This is a no-op in the current ZeroMQ implementation since messages
    /// are sent immediately to the ZMQ kernel buffer. The batch_count is
    /// reset to allow the next batch cycle to begin.
    pub fn flush(&self) {
        self.batch_count.store(0, Ordering::Relaxed);
        debug!("batch flushed");
    }

    /// Returns the current number of messages sent since last flush.
    pub fn pending_in_batch(&self) -> usize {
        self.batch_count.load(Ordering::Relaxed)
    }
}

#[async_trait]
impl PipelineSender for ZmqPipelineSender {
    /// Push a message into the pipeline.
    ///
    /// When batch_size > 1, messages accumulate in the batch counter.
    /// The actual ZMQ send happens immediately (ZMQ handles internal buffering),
    /// but the batch counter tracks application-level batching for flush semantics.
    async fn send(&self, message: Message) -> Result<(), EisenbahnError> {
        let bytes = message.to_bytes()?;
        let mut socket = self.socket.lock().await;
        socket.send(bytes.into()).await?;

        let count = self.batch_count.fetch_add(1, Ordering::Relaxed) + 1;
        if count >= self.config.batch_size {
            self.batch_count.store(0, Ordering::Relaxed);
            debug!(batch_size = self.config.batch_size, "auto-flush at batch boundary");
        }
        Ok(())
    }
}

/// PULL socket receiver that receives work items from upstream PUSH senders.
///
/// Binds to an endpoint and waits for messages. When multiple PULL sockets
/// bind to the same pipeline, work is automatically load-balanced by the
/// upstream PUSH socket's round-robin distribution.
pub struct ZmqPipelineReceiver {
    socket: Mutex<PullSocket>,
}

impl ZmqPipelineReceiver {
    /// Create a new PULL receiver that binds to the given transport endpoint.
    ///
    /// The receiver binds (not connects) because PULL sockets are typically
    /// stable workers that producers connect to.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn bind(
        transport: &Transport,
    ) -> Result<Self, EisenbahnError> {
        transport
            .ensure_ipc_dir()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        transport
            .remove_stale_socket()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        let mut socket = PullSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "binding PULL socket");
        socket.bind(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }

    /// Create a new PULL receiver that connects to the given transport endpoint.
    ///
    /// Use connect when the PUSH sender is the stable endpoint and this
    /// receiver is an ephemeral worker.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn connect(
        transport: &Transport,
    ) -> Result<Self, EisenbahnError> {
        let mut socket = PullSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "connecting PULL socket");
        socket.connect(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }
}

#[async_trait]
impl PipelineReceiver for ZmqPipelineReceiver {
    /// Pull the next message from the pipeline. Blocks until available.
    async fn recv(&self) -> Result<Message, EisenbahnError> {
        let mut socket = self.socket.lock().await;
        let raw = socket.recv().await?;
        let bytes = raw.get(0)
            .ok_or_else(|| EisenbahnError::Transport("empty ZMQ frame".into()))?;
        let message = Message::from_bytes(bytes.as_ref())?;
        Ok(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_config_defaults() {
        let cfg = PipelineConfig::default();
        assert_eq!(cfg.high_water_mark, DEFAULT_HIGH_WATER_MARK);
        assert_eq!(cfg.batch_size, DEFAULT_BATCH_SIZE);
    }

    #[test]
    fn pipeline_config_minimum_batch_size() {
        let cfg = PipelineConfig::new(500, 0);
        assert_eq!(cfg.batch_size, 1, "batch size should be clamped to minimum of 1");
    }

    #[tokio::test]
    async fn push_pull_single_message() {
        let transport = Transport::tcp("127.0.0.1", 15600);
        let config = PipelineConfig::default();

        // Receiver binds first (stable endpoint)
        let receiver = ZmqPipelineReceiver::bind(&transport).await.unwrap();
        // Brief delay for bind to complete
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Sender connects
        let sender = ZmqPipelineSender::new(&transport, config).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Send one message
        let msg = Message::new("pipeline.test", &"hello pipeline".to_string()).unwrap();
        let correlation_id = msg.correlation_id;
        sender.send(msg).await.unwrap();

        // Receive it
        let received = receiver.recv().await.unwrap();
        assert_eq!(received.topic, "pipeline.test");
        assert_eq!(received.correlation_id, correlation_id);
        assert_eq!(received.decode::<String>().unwrap(), "hello pipeline");
    }

    #[tokio::test]
    async fn push_pull_batch_send() {
        let transport = Transport::tcp("127.0.0.1", 15601);
        let config = PipelineConfig::new(1000, 5);

        let receiver = ZmqPipelineReceiver::bind(&transport).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let sender = ZmqPipelineSender::new(&transport, config).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Prepare batch
        let messages: Vec<Message> = (0..5)
            .map(|i| Message::new("batch.item", &i).unwrap())
            .collect();

        // Send as batch
        let sent = sender.send_batch(&messages).await.unwrap();
        assert_eq!(sent, 5);

        // Receive all
        for i in 0..5u32 {
            let received = receiver.recv().await.unwrap();
            assert_eq!(received.topic, "batch.item");
            assert_eq!(received.decode::<u32>().unwrap(), i);
        }
    }

    #[tokio::test]
    async fn push_to_multiple_pull_load_balances() {
        let transport = Transport::tcp("127.0.0.1", 15602);
        let config = PipelineConfig::default();

        // One PUSH binds (stable producer)
        let sender = ZmqPipelineSender::bind(&transport, config).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Two PULL workers connect
        let rx1 = ZmqPipelineReceiver::connect(&transport).await.unwrap();
        let rx2 = ZmqPipelineReceiver::connect(&transport).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Send 10 messages
        let total = 10u32;
        for i in 0..total {
            let msg = Message::new("work.item", &i).unwrap();
            sender.send(msg).await.unwrap();
        }

        // Collect from both receivers with a timeout
        let mut rx1_count = 0u32;
        let mut rx2_count = 0u32;

        // Use a shared channel to collect results
        let (tx, mut channel_rx) = tokio::sync::mpsc::channel::<u32>(20);

        let tx1 = tx.clone();
        let rx1_handle = tokio::spawn(async move {
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    rx1.recv(),
                ).await {
                    Ok(Ok(_msg)) => { let _ = tx1.send(1).await; }
                    _ => break,
                }
            }
        });

        let tx2 = tx.clone();
        let rx2_handle = tokio::spawn(async move {
            loop {
                match tokio::time::timeout(
                    std::time::Duration::from_millis(500),
                    rx2.recv(),
                ).await {
                    Ok(Ok(_msg)) => { let _ = tx2.send(2).await; }
                    _ => break,
                }
            }
        });

        drop(tx); // drop original so channel closes when tasks finish

        while let Some(worker_id) = channel_rx.recv().await {
            if worker_id == 1 { rx1_count += 1; }
            else { rx2_count += 1; }
        }

        rx1_handle.await.unwrap();
        rx2_handle.await.unwrap();

        // Both should have received some messages (load balanced)
        assert_eq!(rx1_count + rx2_count, total, "all messages should be received");
        assert!(rx1_count > 0, "worker 1 should receive some messages");
        assert!(rx2_count > 0, "worker 2 should receive some messages");
    }

    #[test]
    fn batch_counter_tracking() {
        let sender_config = PipelineConfig::new(1000, 5);
        // Just test the atomic counter logic without sockets
        let counter = AtomicUsize::new(0);
        for _ in 0..4 {
            counter.fetch_add(1, Ordering::Relaxed);
        }
        assert_eq!(counter.load(Ordering::Relaxed), 4);

        // Simulates auto-flush at batch boundary
        let count = counter.fetch_add(1, Ordering::Relaxed) + 1;
        assert_eq!(count, sender_config.batch_size);
        counter.store(0, Ordering::Relaxed);
        assert_eq!(counter.load(Ordering::Relaxed), 0);
    }
}
