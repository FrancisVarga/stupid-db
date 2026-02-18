//! Request/reply infrastructure using ZeroMQ DEALER/ROUTER sockets.
//!
//! Provides typed, correlation-id-matched request/reply over ZeroMQ:
//! - [`ZmqRequestClient`] wraps a DEALER socket for sending requests
//! - [`ZmqRequestServer`] wraps a ROUTER socket for receiving and replying
//! - [`ReplyToken`] is an opaque handle carrying the ZMQ identity frame
//!
//! ## Framing (zeromq-rs 0.4)
//!
//! zeromq-rs ROUTER pushes peer identity as first frame on recv and pops it
//! on send. DEALER sends/receives raw application frames. So:
//! - DEALER sends: `[topic, envelope]`
//! - ROUTER receives: `[identity, topic, envelope]`
//! - ROUTER sends: `[identity, topic, envelope]`
//! - DEALER receives: `[topic, envelope]`

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::{mpsc, oneshot, Mutex};
use tracing::{debug, info, instrument, warn};
use uuid::Uuid;
use zeromq::prelude::*;
use zeromq::{DealerSocket, RouterSocket, ZmqMessage};

use crate::error::EisenbahnError;
use crate::message::Message;
use crate::traits::{RequestHandler, RequestSender};
use crate::transport::Transport;

/// Opaque token carrying the ZMQ routing identity bytes.
///
/// When the ROUTER receives a request, it extracts the peer identity frame.
/// This token must be passed back to [`ZmqRequestServer::send_reply`] so the
/// reply is routed to the correct DEALER client.
#[derive(Debug, Clone)]
pub struct ReplyToken {
    identity: Vec<u8>,
}

/// Represents a pending reply: either a one-shot or a streaming channel.
enum PendingReply {
    Single(oneshot::Sender<Result<Message, EisenbahnError>>),
    Stream(mpsc::Sender<Result<Message, EisenbahnError>>),
}

/// Internal command sent from the public API to the background event loop.
struct SendCommand {
    zmq_msg: ZmqMessage,
}

/// ZeroMQ DEALER-socket client for issuing requests and awaiting replies.
///
/// The DEALER socket is owned entirely by a background task that alternates
/// between sending outbound requests (received via an mpsc channel) and
/// receiving inbound replies (dispatched by `correlation_id`). This avoids
/// mutex contention between send and recv paths.
pub struct ZmqRequestClient {
    send_tx: mpsc::Sender<SendCommand>,
    pending: Arc<Mutex<HashMap<Uuid, PendingReply>>>,
    _loop_handle: tokio::task::JoinHandle<()>,
}

impl ZmqRequestClient {
    /// Connect a DEALER socket to a ROUTER endpoint.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn connect(transport: &Transport) -> Result<Self, EisenbahnError> {
        let mut socket = DealerSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "connecting DEALER socket");
        socket.connect(&endpoint).await?;

        let pending: Arc<Mutex<HashMap<Uuid, PendingReply>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let (send_tx, send_rx) = mpsc::channel::<SendCommand>(256);

        let loop_pending = Arc::clone(&pending);
        let loop_handle = tokio::spawn(async move {
            Self::event_loop(socket, send_rx, loop_pending).await;
        });

        Ok(Self {
            send_tx,
            pending,
            _loop_handle: loop_handle,
        })
    }

    /// Single-threaded event loop owning the DEALER socket.
    ///
    /// Uses `tokio::select!` to multiplex sends and receives on the same
    /// socket without mutex contention.
    async fn event_loop(
        mut socket: DealerSocket,
        mut send_rx: mpsc::Receiver<SendCommand>,
        pending: Arc<Mutex<HashMap<Uuid, PendingReply>>>,
    ) {
        loop {
            tokio::select! {
                // Outbound: send a request
                Some(cmd) = send_rx.recv() => {
                    if let Err(e) = socket.send(cmd.zmq_msg).await {
                        warn!(error = %e, "DEALER send failed");
                    }
                }
                // Inbound: receive a reply
                result = socket.recv() => {
                    match result {
                        Ok(zmq_msg) => {
                            Self::dispatch_reply(&pending, zmq_msg).await;
                        }
                        Err(e) => {
                            debug!(error = %e, "DEALER recv loop ending");
                            break;
                        }
                    }
                }
                else => break,
            }
        }
    }

    /// Route an inbound reply to the correct pending caller.
    async fn dispatch_reply(
        pending: &Mutex<HashMap<Uuid, PendingReply>>,
        zmq_msg: ZmqMessage,
    ) {
        let frames: Vec<_> = zmq_msg.iter().collect();

        // Skip leading empty delimiter frames (DEALER may receive them
        // depending on the ROUTER's reply framing).
        let data_frames: Vec<_> = frames
            .iter()
            .skip_while(|f| f.as_ref().is_empty())
            .collect();

        if data_frames.len() < 2 {
            warn!(
                raw_frame_count = frames.len(),
                data_frame_count = data_frames.len(),
                "unexpected frame count on DEALER recv"
            );
            return;
        }

        let envelope_bytes = data_frames[1].as_ref();
        let message = match Message::from_bytes(envelope_bytes) {
            Ok(m) => m,
            Err(e) => {
                warn!(error = %e, "failed to decode reply envelope");
                return;
            }
        };

        let cid = message.correlation_id;
        let mut map = pending.lock().await;

        if let Some(entry) = map.get(&cid) {
            match entry {
                PendingReply::Single(_) => {
                    if let Some(PendingReply::Single(tx)) = map.remove(&cid) {
                        let _ = tx.send(Ok(message));
                    }
                }
                PendingReply::Stream(tx) => {
                    let is_done = message.topic.ends_with(".done");
                    let _ = tx.send(Ok(message)).await;
                    if is_done {
                        map.remove(&cid);
                    }
                }
            }
        } else {
            debug!(correlation_id = %cid, "received reply for unknown correlation_id");
        }
    }

    /// Send a request and return a receiver for streaming replies.
    ///
    /// The server can send multiple reply messages sharing the same `correlation_id`.
    /// The stream ends when a message with a topic ending in `.done` is received.
    pub async fn request_stream(
        &self,
        msg: Message,
    ) -> Result<mpsc::Receiver<Result<Message, EisenbahnError>>, EisenbahnError> {
        let cid = msg.correlation_id;
        let (tx, rx) = mpsc::channel(64);

        {
            let mut map = self.pending.lock().await;
            map.insert(cid, PendingReply::Stream(tx));
        }

        self.enqueue_send(&msg).await?;
        debug!(correlation_id = %cid, topic = %msg.topic, "sent streaming request");
        Ok(rx)
    }

    /// Serialize the message and enqueue it for the background event loop.
    async fn enqueue_send(&self, msg: &Message) -> Result<(), EisenbahnError> {
        let envelope_bytes = msg.to_bytes()?;
        let mut zmq_msg = ZmqMessage::from(msg.topic.as_str());
        zmq_msg.push_back(envelope_bytes.into());

        self.send_tx
            .send(SendCommand { zmq_msg })
            .await
            .map_err(|_| EisenbahnError::Transport("client event loop closed".into()))?;
        Ok(())
    }
}

#[async_trait]
impl RequestSender for ZmqRequestClient {
    /// Send a request and wait for a single reply matched by `correlation_id`.
    ///
    /// Returns `EisenbahnError::Timeout` if no reply arrives within `timeout`.
    async fn request(&self, msg: Message, timeout_dur: Duration) -> Result<Message, EisenbahnError> {
        let cid = msg.correlation_id;
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(cid, PendingReply::Single(tx));
        }

        self.enqueue_send(&msg).await?;
        debug!(correlation_id = %cid, topic = %msg.topic, "sent request");

        match tokio::time::timeout(timeout_dur, rx).await {
            Ok(Ok(result)) => result,
            Ok(Err(_)) => {
                self.pending.lock().await.remove(&cid);
                Err(EisenbahnError::Transport(
                    "reply channel closed unexpectedly".into(),
                ))
            }
            Err(_) => {
                self.pending.lock().await.remove(&cid);
                Err(EisenbahnError::Timeout(timeout_dur))
            }
        }
    }
}

/// ZeroMQ ROUTER-socket server for receiving requests and sending replies.
///
/// Binds a ROUTER socket. Each received message includes the peer identity,
/// which is wrapped in a [`ReplyToken`] for routing the reply back.
pub struct ZmqRequestServer {
    socket: Mutex<RouterSocket>,
}

impl ZmqRequestServer {
    /// Bind a ROUTER socket on the given transport endpoint.
    #[instrument(skip_all, fields(endpoint = %transport))]
    pub async fn bind(transport: &Transport) -> Result<Self, EisenbahnError> {
        transport
            .ensure_ipc_dir()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        transport
            .remove_stale_socket()
            .map_err(|e| EisenbahnError::Transport(e.to_string()))?;
        let mut socket = RouterSocket::new();
        let endpoint = transport.endpoint();
        info!(endpoint = %endpoint, "binding ROUTER socket");
        socket.bind(&endpoint).await?;
        Ok(Self {
            socket: Mutex::new(socket),
        })
    }
}

#[async_trait]
impl RequestHandler for ZmqRequestServer {
    /// Receive the next request from any connected DEALER client.
    ///
    /// Returns a [`ReplyToken`] (holding the peer identity) and the decoded [`Message`].
    async fn recv_request(&self) -> Result<(ReplyToken, Message), EisenbahnError> {
        let mut socket = self.socket.lock().await;
        let zmq_msg = socket.recv().await?;

        // ROUTER recv frames: [identity, ...data_frames]
        // The identity is prepended by zeromq-rs. Remaining frames are
        // whatever the DEALER sent: [topic, envelope].
        let frames: Vec<_> = zmq_msg.iter().collect();

        if frames.len() < 2 {
            return Err(EisenbahnError::Transport(format!(
                "expected at least 2 frames from ROUTER, got {}",
                frames.len()
            )));
        }

        let identity = frames[0].as_ref().to_vec();

        // Skip identity and any empty delimiter frames to find [topic, envelope].
        let data_frames: Vec<_> = frames[1..]
            .iter()
            .skip_while(|f| f.as_ref().is_empty())
            .collect();

        if data_frames.len() < 2 {
            return Err(EisenbahnError::Transport(format!(
                "expected [topic, envelope] after identity, got {} data frames",
                data_frames.len()
            )));
        }

        let envelope_bytes = data_frames[1].as_ref();
        let message = Message::from_bytes(envelope_bytes)?;

        debug!(
            correlation_id = %message.correlation_id,
            topic = %message.topic,
            "received request"
        );

        Ok((ReplyToken { identity }, message))
    }

    /// Send a reply to the client identified by the [`ReplyToken`].
    ///
    /// Frames sent: `[identity, topic, envelope]`
    /// ROUTER pops identity and routes the remaining frames to the peer.
    async fn send_reply(
        &self,
        token: ReplyToken,
        reply: Message,
    ) -> Result<(), EisenbahnError> {
        let envelope_bytes = reply.to_bytes()?;

        let mut zmq_msg = ZmqMessage::from(token.identity);
        zmq_msg.push_back(reply.topic.as_bytes().to_vec().into());
        zmq_msg.push_back(envelope_bytes.into());

        let mut socket = self.socket.lock().await;
        socket.send(zmq_msg).await?;

        debug!(
            correlation_id = %reply.correlation_id,
            topic = %reply.topic,
            "sent reply"
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_token_clone() {
        let token = ReplyToken {
            identity: vec![1, 2, 3],
        };
        let cloned = token.clone();
        assert_eq!(token.identity, cloned.identity);
    }
}
