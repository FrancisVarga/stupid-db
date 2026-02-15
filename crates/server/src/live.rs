use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::response::IntoResponse;
use futures::{SinkExt, StreamExt};
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use tokio::sync::{broadcast, RwLock};
use tracing::{error, info, warn};

use crate::state::AppState;

// ── WebSocket Messages ──────────────────────────────────────────

#[derive(Serialize)]
struct WsMessage<T: Serialize> {
    #[serde(rename = "type")]
    msg_type: &'static str,
    data: T,
}

fn ws_json<T: Serialize>(msg_type: &'static str, data: T) -> String {
    serde_json::to_string(&WsMessage { msg_type, data }).unwrap_or_default()
}

// ── WebSocket Handler ───────────────────────────────────────────

pub async fn ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.broadcast.subscribe();

    // Send current stats as initial state.
    let initial = build_stats_message(&state).await;
    if sender.send(Message::Text(initial.into())).await.is_err() {
        return;
    }

    // Forward broadcast messages to this client.
    let send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Consume incoming messages (pings, close frames) but ignore content.
    let recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if matches!(msg, Message::Close(_)) {
                break;
            }
        }
    });

    // Wait for either task to finish (client disconnect or broadcast channel close).
    tokio::select! {
        _ = send_task => {},
        _ = recv_task => {},
    }
}

async fn build_stats_message(state: &AppState) -> String {
    let graph = state.graph.read().await;
    let gs = graph.stats();
    let segment_ids = state.segment_ids.read().await;

    ws_json("stats", serde_json::json!({
        "doc_count": state.doc_count.load(Ordering::Relaxed),
        "segment_count": segment_ids.len(),
        "node_count": gs.node_count,
        "edge_count": gs.edge_count,
        "nodes_by_type": gs.nodes_by_type,
        "edges_by_type": gs.edges_by_type,
    }))
}

// ── Segment Watcher ─────────────────────────────────────────────

pub async fn start_segment_watcher(
    data_dir: PathBuf,
    graph: crate::state::SharedGraph,
    knowledge: stupid_compute::SharedKnowledgeState,
    pipeline: crate::state::SharedPipeline,
    segment_ids: Arc<RwLock<Vec<String>>>,
    doc_count: Arc<std::sync::atomic::AtomicU64>,
    broadcast_tx: broadcast::Sender<String>,
) {
    let segments_dir = data_dir.join("segments");
    if !segments_dir.exists() {
        // Create segments dir if it doesn't exist so we can watch it.
        if let Err(e) = std::fs::create_dir_all(&segments_dir) {
            error!("Cannot create segments directory for watching: {}", e);
            return;
        }
    }

    let (notify_tx, mut notify_rx) = tokio::sync::mpsc::channel::<PathBuf>(256);

    // Capture the Tokio runtime handle BEFORE spawning the OS thread,
    // since Handle::current() requires an active Tokio context.
    let rt = tokio::runtime::Handle::current();

    // Start the filesystem watcher in a blocking thread.
    let watch_dir = segments_dir.clone();
    std::thread::spawn(move || {
        let tx = notify_tx;

        let mut watcher: RecommendedWatcher = match notify::recommended_watcher(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if matches!(
                        event.kind,
                        EventKind::Create(_) | EventKind::Modify(_)
                    ) {
                        for path in event.paths {
                            if path.file_name().map(|n| n == "documents.dat").unwrap_or(false) {
                                let tx = tx.clone();
                                rt.spawn(async move {
                                    let _ = tx.send(path).await;
                                });
                            }
                        }
                    }
                }
            },
        ) {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to create file watcher: {}", e);
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::Recursive) {
            tracing::error!("Failed to watch {}: {}", watch_dir.display(), e);
            return;
        }

        info!("Watching {} for new segments", watch_dir.display());

        // Keep the watcher alive.
        loop {
            std::thread::sleep(std::time::Duration::from_secs(60));
        }
    });

    // Process new segment notifications with debouncing.
    let mut pending: HashSet<String> = HashSet::new();
    let mut debounce_timer = tokio::time::interval(tokio::time::Duration::from_secs(3));
    debounce_timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            Some(path) = notify_rx.recv() => {
                // Extract segment_id from path.
                if let Some(seg_id) = extract_segment_id(&path, &segments_dir) {
                    // Check if already loaded.
                    let known = segment_ids.read().await;
                    if !known.contains(&seg_id) {
                        pending.insert(seg_id);
                    }
                }
            }
            _ = debounce_timer.tick() => {
                if pending.is_empty() {
                    continue;
                }

                let new_segments: Vec<String> = pending.drain().collect();
                info!("Detected {} new segment(s): {:?}", new_segments.len(), &new_segments);

                // Ingest new segments into the graph and collect docs for pipeline.
                let mut total_new_docs = 0u64;
                let mut new_docs: Vec<stupid_core::Document> = Vec::new();
                {
                    let mut graph_lock = graph.write().await;
                    for seg_id in &new_segments {
                        match stupid_segment::reader::SegmentReader::open(&data_dir, seg_id) {
                            Ok(reader) => {
                                let mut seg_docs = 0u64;
                                for doc_result in reader.iter() {
                                    match doc_result {
                                        Ok(doc) => {
                                            stupid_connector::entity_extract::EntityExtractor::extract(
                                                &doc, &mut graph_lock, seg_id,
                                            );
                                            new_docs.push(doc);
                                            seg_docs += 1;
                                        }
                                        Err(e) => {
                                            warn!("Bad document in new segment '{}': {}", seg_id, e);
                                        }
                                    }
                                }
                                total_new_docs += seg_docs;
                                info!("  Ingested {} docs from new segment '{}'", seg_docs, seg_id);
                            }
                            Err(e) => {
                                warn!("Failed to read new segment '{}': {}", seg_id, e);
                            }
                        }
                    }
                }

                // Update segment tracking.
                {
                    let mut ids = segment_ids.write().await;
                    for seg_id in &new_segments {
                        if !ids.contains(seg_id) {
                            ids.push(seg_id.clone());
                        }
                    }
                }
                doc_count.fetch_add(total_new_docs, Ordering::Relaxed);

                // Recompute algorithms on the updated graph into shared KnowledgeState.
                info!("Recomputing graph algorithms after ingesting {} new docs...", total_new_docs);
                {
                    let graph_read = graph.read().await;
                    let pagerank = stupid_compute::algorithms::pagerank::pagerank_default(&graph_read);
                    let degrees = stupid_compute::algorithms::degree::degree_centrality(&graph_read);
                    let communities = stupid_compute::algorithms::communities::label_propagation_default(&graph_read);
                    drop(graph_read);
                    let mut state = knowledge.write().unwrap();
                    state.pagerank = pagerank;
                    state.degrees = degrees;
                    state.communities = communities;
                }
                info!("Compute algorithms updated after live ingestion");

                // Run pipeline on new docs (hot_connect + warm_compute).
                if !new_docs.is_empty() {
                    let mut pipe = pipeline.lock().unwrap();
                    let mut state = knowledge.write().unwrap();
                    pipe.hot_connect(&new_docs, &mut state);
                    pipe.warm_compute(&mut state, &new_docs);
                    info!(
                        "Pipeline updated: {} anomalies, {} trends, {} clusters",
                        state.anomalies.len(), state.trends.len(), state.clusters.len()
                    );
                }

                // Broadcast updated stats to all connected WebSocket clients.
                let graph_read = graph.read().await;
                let gs = graph_read.stats();
                let seg_count = segment_ids.read().await.len();
                drop(graph_read);

                let stats_msg = ws_json("stats", serde_json::json!({
                    "doc_count": doc_count.load(Ordering::Relaxed),
                    "segment_count": seg_count,
                    "node_count": gs.node_count,
                    "edge_count": gs.edge_count,
                    "nodes_by_type": gs.nodes_by_type,
                    "edges_by_type": gs.edges_by_type,
                }));

                let _ = broadcast_tx.send(stats_msg);

                // Broadcast a segment update notification.
                let seg_msg = ws_json("segments", serde_json::json!({
                    "new_segments": new_segments,
                    "total": seg_count,
                }));
                let _ = broadcast_tx.send(seg_msg);
            }
        }
    }
}

/// Extract segment_id from a documents.dat path relative to segments_dir.
fn extract_segment_id(path: &std::path::Path, segments_dir: &std::path::Path) -> Option<String> {
    let parent = path.parent()?;
    let rel = parent.strip_prefix(segments_dir).ok()?;
    let seg_id = rel.to_str()?;
    Some(seg_id.replace('\\', "/"))
}
