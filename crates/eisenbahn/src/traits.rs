use std::sync::Arc;

use async_trait::async_trait;

use crate::error::EisenbahnError;
use crate::message::Message;

/// Publishes messages to one or more subscribers via PUB/SUB pattern.
///
/// Publishers send topic-filtered messages to all connected subscribers.
/// This is the broadcast side of the fan-out pattern.
#[async_trait]
pub trait EventPublisher: Send + Sync {
    /// Publish a message. Subscribers filter by the message's topic.
    async fn publish(&self, message: Message) -> Result<(), EisenbahnError>;
}

/// Blanket implementation so `Arc<dyn EventPublisher>` can be used directly.
#[async_trait]
impl<T: EventPublisher + ?Sized> EventPublisher for Arc<T> {
    async fn publish(&self, message: Message) -> Result<(), EisenbahnError> {
        (**self).publish(message).await
    }
}

/// Subscribes to messages matching topic filters via PUB/SUB pattern.
///
/// Subscribers connect to a publisher and receive messages whose topics
/// match the subscribed prefixes.
#[async_trait]
pub trait EventSubscriber: Send + Sync {
    /// Subscribe to messages with topics matching the given prefix.
    async fn subscribe(&self, topic_prefix: &str) -> Result<(), EisenbahnError>;

    /// Receive the next message. Blocks until a message is available.
    async fn recv(&self) -> Result<Message, EisenbahnError>;
}

/// Sends work items through a PUSH/PULL pipeline.
///
/// Pipeline senders distribute work across connected receivers
/// in a round-robin fashion for load balancing.
#[async_trait]
pub trait PipelineSender: Send + Sync {
    /// Push a message into the pipeline.
    async fn send(&self, message: Message) -> Result<(), EisenbahnError>;
}

/// Receives work items from a PUSH/PULL pipeline.
///
/// Pipeline receivers pull work items that were distributed by senders.
/// Multiple receivers on the same pipeline get load-balanced delivery.
#[async_trait]
pub trait PipelineReceiver: Send + Sync {
    /// Pull the next message from the pipeline. Blocks until available.
    async fn recv(&self) -> Result<Message, EisenbahnError>;
}
