use async_trait::async_trait;
use tokio::sync::Mutex;
use tracing::{debug, info, instrument};
use zeromq::prelude::*;
use zeromq::{PubSocket, SubSocket, ZmqMessage};

use crate::error::EisenbahnError;
use crate::message::Message;
use crate::traits::{EventPublisher, EventSubscriber};
use crate::transport::Transport;

/// ZeroMQ PUB socket publisher that connects to the broker's frontend.
///
/// Messages are sent as two-frame ZMQ messages:
/// 1. Topic string (used by SUB sockets for prefix filtering)
/// 2. MessagePack-encoded [`Message`] envelope
///
/// The publisher connects to the broker's frontend (SUB socket),
/// which subscribes to all topics and forwards them to the backend (PUB socket).
pub struct ZmqPublisher {
    socket: Mutex<PubSocket>,
}

impl ZmqPublisher {
    /// Create a new publisher that connects to the broker's frontend endpoint.
    ///
    /// # Arguments
    /// * `transport` - The broker frontend endpoint (where the broker's SUB socket binds).
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn connect(transport: &Transport) -> Result<Self, EisenbahnError> {
        let mut socket = PubSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "connecting PUB socket to broker frontend");
        socket.connect(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }

    /// Create a new publisher that binds to the given endpoint.
    ///
    /// Use this for direct PUB/SUB without a broker (publisher binds, subscribers connect).
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn bind(transport: &Transport) -> Result<Self, EisenbahnError> {
        let mut socket = PubSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "binding PUB socket");
        socket.bind(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }
}

#[async_trait]
impl EventPublisher for ZmqPublisher {
    /// Publish a message as a two-frame ZMQ message: [topic, envelope].
    ///
    /// The topic frame enables subscriber-side prefix filtering.
    /// The envelope frame contains the full MessagePack-serialized [`Message`].
    async fn publish(&self, message: Message) -> Result<(), EisenbahnError> {
        let topic = message.topic.clone();
        let envelope_bytes = message.to_bytes()?;

        // Build a two-frame ZMQ message: [topic, envelope]
        let mut zmq_msg = ZmqMessage::from(topic.as_str());
        zmq_msg.push_back(envelope_bytes.into());

        let mut socket = self.socket.lock().await;
        socket.send(zmq_msg).await?;

        debug!(topic = %topic, "published message");
        Ok(())
    }
}

/// ZeroMQ SUB socket subscriber that connects to the broker's backend.
///
/// Receives two-frame ZMQ messages:
/// 1. Topic string (used for prefix matching)
/// 2. MessagePack-encoded [`Message`] envelope
///
/// The subscriber connects to the broker's backend (PUB socket),
/// which forwards messages received from publishers on the frontend.
pub struct ZmqSubscriber {
    socket: Mutex<SubSocket>,
}

impl ZmqSubscriber {
    /// Create a new subscriber that connects to the broker's backend endpoint.
    ///
    /// # Arguments
    /// * `transport` - The broker backend endpoint (where the broker's PUB socket binds).
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn connect(transport: &Transport) -> Result<Self, EisenbahnError> {
        let mut socket = SubSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "connecting SUB socket to broker backend");
        socket.connect(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }

    /// Create a new subscriber that connects directly to a publisher (no broker).
    ///
    /// Use this for direct PUB/SUB without a broker.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn connect_direct(transport: &Transport) -> Result<Self, EisenbahnError> {
        Self::connect(transport).await
    }
}

#[async_trait]
impl EventSubscriber for ZmqSubscriber {
    /// Subscribe to messages with topics matching the given prefix.
    ///
    /// An empty string subscribes to all topics.
    /// Multiple subscriptions can be active simultaneously.
    async fn subscribe(&self, topic_prefix: &str) -> Result<(), EisenbahnError> {
        let mut socket = self.socket.lock().await;
        socket.subscribe(topic_prefix).await?;
        info!(topic_prefix = %topic_prefix, "subscribed to topic prefix");
        Ok(())
    }

    /// Receive the next message. Blocks until a message matching a subscription arrives.
    ///
    /// Expects a two-frame ZMQ message: [topic, envelope].
    /// The envelope (second frame) is deserialized into a [`Message`].
    async fn recv(&self) -> Result<Message, EisenbahnError> {
        let mut socket = self.socket.lock().await;
        let zmq_msg = socket.recv().await?;

        // The message may be a single frame (topic+data combined by ZMQ)
        // or two frames [topic, envelope]. We need to handle both cases.
        //
        // In zeromq-rs, PUB/SUB sends the topic as a prefix of the first frame
        // for single-frame messages, or as separate frames for multi-frame messages.
        // Since we explicitly send two frames, we expect frame[1] to be our envelope.
        let frames: Vec<_> = zmq_msg.iter().collect();

        if frames.len() >= 2 {
            // Multi-frame: [topic, envelope]
            let envelope_bytes = frames[1].as_ref();
            let message = Message::from_bytes(envelope_bytes)?;
            debug!(topic = %message.topic, "received message");
            Ok(message)
        } else if !frames.is_empty() {
            // Single-frame fallback: the entire frame is the envelope
            // This shouldn't happen with our publisher, but handle gracefully.
            let envelope_bytes = frames[0].as_ref();
            let message = Message::from_bytes(envelope_bytes)?;
            debug!(topic = %message.topic, "received single-frame message");
            Ok(message)
        } else {
            Err(EisenbahnError::Transport("empty ZMQ message".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zmq_message_two_frame_construction() {
        // Verify our two-frame message construction works correctly.
        let topic = "eisenbahn.test.topic";
        let payload_bytes = b"test-payload";

        let mut msg = ZmqMessage::from(topic);
        msg.push_back(payload_bytes.to_vec().into());

        let frames: Vec<_> = msg.iter().collect();
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].as_ref(), topic.as_bytes());
        assert_eq!(frames[1].as_ref(), payload_bytes);
    }

    #[tokio::test]
    async fn direct_pub_sub_roundtrip() {
        // Direct PUB/SUB without broker: publisher binds, subscriber connects.
        let transport = Transport::tcp("127.0.0.1", 15700);

        // Publisher binds
        let publisher = ZmqPublisher::bind(&transport).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Subscriber connects and subscribes
        let subscriber = ZmqSubscriber::connect(&transport).await.unwrap();
        subscriber.subscribe("eisenbahn.test").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Publish a message
        let msg = Message::new("eisenbahn.test.hello", &"world".to_string()).unwrap();
        let correlation_id = msg.correlation_id;
        publisher.publish(msg).await.unwrap();

        // Receive with timeout
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            subscriber.recv(),
        )
        .await
        .expect("timed out waiting for message")
        .unwrap();

        assert_eq!(received.topic, "eisenbahn.test.hello");
        assert_eq!(received.correlation_id, correlation_id);
        assert_eq!(received.decode::<String>().unwrap(), "world");
    }

    #[tokio::test]
    async fn topic_filtering_works() {
        // Subscriber should only receive messages matching its subscription prefix.
        let transport = Transport::tcp("127.0.0.1", 15701);

        let publisher = ZmqPublisher::bind(&transport).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let subscriber = ZmqSubscriber::connect(&transport).await.unwrap();
        // Subscribe only to anomaly topics
        subscriber.subscribe("eisenbahn.anomaly").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Publish an anomaly message (should be received)
        let anomaly_msg =
            Message::new("eisenbahn.anomaly.detected", &"anomaly payload".to_string()).unwrap();
        let anomaly_id = anomaly_msg.correlation_id;
        publisher.publish(anomaly_msg).await.unwrap();

        // Publish an ingest message (should be filtered out)
        let ingest_msg =
            Message::new("eisenbahn.ingest.complete", &"ingest payload".to_string()).unwrap();
        publisher.publish(ingest_msg).await.unwrap();

        // We should receive only the anomaly message
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(2),
            subscriber.recv(),
        )
        .await
        .expect("timed out")
        .unwrap();

        assert_eq!(received.topic, "eisenbahn.anomaly.detected");
        assert_eq!(received.correlation_id, anomaly_id);

        // Verify no more messages arrive (the ingest one was filtered)
        let timeout_result = tokio::time::timeout(
            std::time::Duration::from_millis(300),
            subscriber.recv(),
        )
        .await;
        assert!(timeout_result.is_err(), "should not receive filtered message");
    }

    #[tokio::test]
    async fn broker_roundtrip() {
        use crate::broker::{BrokerConfig as BrokerSocketConfig, EventBroker};

        // Set up broker with TCP endpoints
        let broker_cfg = BrokerSocketConfig::tcp("127.0.0.1", 15710, 15711, 15712);

        // Start broker in background
        let broker_handle = tokio::spawn({
            let cfg = broker_cfg.clone();
            async move {
                let broker = EventBroker::new(cfg);
                broker.run().await
            }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // Publisher connects to broker frontend (where SUB socket binds)
        let frontend_transport = Transport::tcp("127.0.0.1", 15710);
        let publisher = ZmqPublisher::connect(&frontend_transport).await.unwrap();

        // Subscriber connects to broker backend (where PUB socket binds)
        let backend_transport = Transport::tcp("127.0.0.1", 15711);
        let subscriber = ZmqSubscriber::connect(&backend_transport).await.unwrap();
        subscriber.subscribe("eisenbahn.").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;

        // Publish a typed event through the broker
        use crate::messages::events::IngestComplete;
        use crate::messages::topics;

        let event = IngestComplete {
            source: "test.parquet".into(),
            record_count: 1000,
            duration_ms: 42,
        };
        let msg = Message::new(topics::INGEST_COMPLETE, &event).unwrap();
        let correlation_id = msg.correlation_id;
        publisher.publish(msg).await.unwrap();

        // Receive through broker
        let received = tokio::time::timeout(
            std::time::Duration::from_secs(3),
            subscriber.recv(),
        )
        .await
        .expect("timed out waiting for broker-forwarded message")
        .unwrap();

        assert_eq!(received.topic, topics::INGEST_COMPLETE);
        assert_eq!(received.correlation_id, correlation_id);

        let decoded: IngestComplete = received.decode().unwrap();
        assert_eq!(decoded.source, "test.parquet");
        assert_eq!(decoded.record_count, 1000);
        assert_eq!(decoded.duration_ms, 42);

        // Clean up: abort the broker
        broker_handle.abort();
    }

    #[tokio::test]
    async fn multiple_subscribers_receive_same_message() {
        // PUB/SUB is fan-out: all subscribers get every matching message.
        let transport = Transport::tcp("127.0.0.1", 15720);

        let publisher = ZmqPublisher::bind(&transport).await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let sub1 = ZmqSubscriber::connect(&transport).await.unwrap();
        let sub2 = ZmqSubscriber::connect(&transport).await.unwrap();
        sub1.subscribe("eisenbahn.").await.unwrap();
        sub2.subscribe("eisenbahn.").await.unwrap();
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let msg = Message::new("eisenbahn.test.fanout", &42u64).unwrap();
        let cid = msg.correlation_id;
        publisher.publish(msg).await.unwrap();

        let r1 = tokio::time::timeout(std::time::Duration::from_secs(2), sub1.recv())
            .await
            .expect("sub1 timed out")
            .unwrap();
        let r2 = tokio::time::timeout(std::time::Duration::from_secs(2), sub2.recv())
            .await
            .expect("sub2 timed out")
            .unwrap();

        assert_eq!(r1.correlation_id, cid);
        assert_eq!(r2.correlation_id, cid);
        assert_eq!(r1.decode::<u64>().unwrap(), 42);
        assert_eq!(r2.decode::<u64>().unwrap(), 42);
    }
}
