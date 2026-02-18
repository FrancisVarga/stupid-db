//! Integration tests for DEALER/ROUTER request/reply pattern.
//!
//! Tests verify correlation-id matching, concurrent requests,
//! timeouts, and streaming replies.

use std::time::Duration;

use stupid_eisenbahn::transport::Transport;
use stupid_eisenbahn::{
    EisenbahnError, Message, RequestHandler, RequestSender, ZmqRequestClient, ZmqRequestServer,
};

const SETTLE: Duration = Duration::from_millis(200);
const TIMEOUT: Duration = Duration::from_secs(5);

#[tokio::test]
async fn single_request_reply() {
    let transport = Transport::tcp("127.0.0.1", 16500);

    // Server binds ROUTER
    let server = ZmqRequestServer::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    // Client connects DEALER
    let client = ZmqRequestClient::connect(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let request_msg = Message::new("service.query", &"ping".to_string()).unwrap();
    let cid = request_msg.correlation_id;

    // Spawn server handler
    let server_handle = tokio::spawn(async move {
        let (token, msg) = server.recv_request().await.unwrap();
        assert_eq!(msg.topic, "service.query");
        assert_eq!(msg.decode::<String>().unwrap(), "ping");

        let reply = Message::with_correlation("service.query.reply", &"pong".to_string(), msg.correlation_id).unwrap();
        server.send_reply(token, reply).await.unwrap();
    });

    // Client sends request
    let reply = client.request(request_msg, TIMEOUT).await.unwrap();
    assert_eq!(reply.correlation_id, cid);
    assert_eq!(reply.topic, "service.query.reply");
    assert_eq!(reply.decode::<String>().unwrap(), "pong");

    server_handle.await.unwrap();
}

#[tokio::test]
async fn concurrent_requests() {
    let transport = Transport::tcp("127.0.0.1", 16510);

    let server = ZmqRequestServer::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let client = ZmqRequestClient::connect(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let num_requests = 5u32;

    // Server handles all requests in a loop
    let server_handle = tokio::spawn(async move {
        for _ in 0..num_requests {
            let (token, msg) = server.recv_request().await.unwrap();
            let value: u32 = msg.decode().unwrap();
            let reply = Message::with_correlation(
                "service.echo.reply",
                &(value * 10),
                msg.correlation_id,
            )
            .unwrap();
            server.send_reply(token, reply).await.unwrap();
        }
    });

    // Fire all requests concurrently
    let client = std::sync::Arc::new(client);
    let mut handles = Vec::new();
    for i in 0..num_requests {
        let c = std::sync::Arc::clone(&client);
        handles.push(tokio::spawn(async move {
            let msg = Message::new("service.echo", &i).unwrap();
            let cid = msg.correlation_id;
            let reply = c.request(msg, TIMEOUT).await.unwrap();
            assert_eq!(reply.correlation_id, cid);
            let value: u32 = reply.decode().unwrap();
            assert_eq!(value, i * 10);
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
    server_handle.await.unwrap();
}

#[tokio::test]
async fn request_timeout() {
    let transport = Transport::tcp("127.0.0.1", 16520);

    // Bind server but never reply
    let _server = ZmqRequestServer::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let client = ZmqRequestClient::connect(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let msg = Message::new("service.black_hole", &"hello".to_string()).unwrap();
    let short_timeout = Duration::from_millis(300);

    let result = client.request(msg, short_timeout).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        EisenbahnError::Timeout(d) => assert_eq!(d, short_timeout),
        other => panic!("expected Timeout error, got: {other}"),
    }
}

#[tokio::test]
async fn streaming_replies() {
    let transport = Transport::tcp("127.0.0.1", 16530);

    let server = ZmqRequestServer::bind(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let client = ZmqRequestClient::connect(&transport).await.unwrap();
    tokio::time::sleep(SETTLE).await;

    let request = Message::new("service.stream", &"start".to_string()).unwrap();
    let cid = request.correlation_id;

    // Server sends 3 data replies + 1 done reply
    let server_handle = tokio::spawn(async move {
        let (token, msg) = server.recv_request().await.unwrap();
        let cid = msg.correlation_id;

        for i in 0u32..3 {
            let reply = Message::with_correlation("service.stream.chunk", &i, cid).unwrap();
            server.send_reply(token.clone(), reply).await.unwrap();
            // Small delay to ensure ordering
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        // Final "done" message
        let done = Message::with_correlation("service.stream.done", &"complete".to_string(), cid).unwrap();
        server.send_reply(token, done).await.unwrap();
    });

    // Client receives streaming replies
    let mut rx = client.request_stream(request).await.unwrap();

    let mut chunks = Vec::new();
    let mut got_done = false;

    while let Some(result) = tokio::time::timeout(TIMEOUT, rx.recv()).await.unwrap() {
        let msg = result.unwrap();
        assert_eq!(msg.correlation_id, cid);

        if msg.topic.ends_with(".done") {
            got_done = true;
            break;
        } else {
            let value: u32 = msg.decode().unwrap();
            chunks.push(value);
        }
    }

    assert!(got_done, "should have received .done message");
    assert_eq!(chunks, vec![0, 1, 2]);

    server_handle.await.unwrap();
}
