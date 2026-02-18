use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

use crate::error::EisenbahnError;
use crate::message::Message;
use crate::reqrep::ReplyToken;

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

/// Sends a request and waits for a reply via DEALER/ROUTER pattern.
///
/// Clients use this trait to issue requests to service workers.
/// Replies are matched by `correlation_id` so multiple requests can
/// be in flight concurrently.
#[async_trait]
pub trait RequestSender: Send + Sync {
    /// Send a request and wait for a reply within the given timeout.
    async fn request(
        &self,
        message: Message,
        timeout: Duration,
    ) -> Result<Message, EisenbahnError>;
}

/// Handles incoming requests and sends replies via ROUTER/DEALER pattern.
///
/// Service workers implement this trait to receive requests from clients
/// and send back replies routed to the correct originating DEALER.
#[async_trait]
pub trait RequestHandler: Send + Sync {
    /// Receive the next request. Returns an opaque reply token and the message.
    async fn recv_request(&self) -> Result<(ReplyToken, Message), EisenbahnError>;

    /// Send a reply back to the client identified by the reply token.
    async fn send_reply(
        &self,
        token: ReplyToken,
        reply: Message,
    ) -> Result<(), EisenbahnError>;
}
