use std::path::Path;

use tracing::info;

// ── Lightweight graph operations for parallel extraction ──────────────
//
// Instead of sending full Documents through a channel, rayon workers
// extract these tiny ops (~50-100 bytes each). The consumer replays
// them into the single-threaded GraphStore.

pub(crate) enum GraphOp {
    /// Upsert a node and add edges from it.
    Node {
        entity_type: stupid_core::EntityType,
        key: String,
        edges: Vec<(stupid_core::EntityType, String, stupid_core::EdgeType)>,
    },
}

/// Extract graph ops from a document (runs in rayon worker, no GraphStore needed).
pub(crate) fn extract_graph_ops(doc: &stupid_core::Document, _seg_id: &str, ops: &mut Vec<GraphOp>) {
    use stupid_core::{EdgeType, EntityType};

    let member_code = match doc.fields.get("memberCode").and_then(|v| v.as_str()) {
        Some(s) if !s.trim().is_empty() && s.trim() != "None" && s.trim() != "null" => s.trim().to_string(),
        _ => return,
    };

    let member_key = format!("member:{}", member_code);
    let mut edges = Vec::new();

    let get = |name: &str| -> Option<String> {
        doc.fields.get(name).and_then(|v| v.as_str()).and_then(|s| {
            let t = s.trim();
            if t.is_empty() || t == "None" || t == "null" || t == "undefined" { None }
            else { Some(t.to_string()) }
        })
    };

    match doc.event_type.as_str() {
        "Login" => {
            if let Some(fp) = get("fingerprint") {
                edges.push((EntityType::Device, format!("device:{}", fp), EdgeType::LoggedInFrom));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
            if let Some(c) = get("currency") {
                edges.push((EntityType::Currency, format!("currency:{}", c), EdgeType::UsesCurrency));
            }
            if let Some(g) = get("rGroup") {
                edges.push((EntityType::VipGroup, format!("vipgroup:{}", g), EdgeType::BelongsToGroup));
            }
            let aff = get("affiliateId").or_else(|| get("affiliateid")).or_else(|| get("affiliateID"));
            if let Some(a) = aff {
                edges.push((EntityType::Affiliate, format!("affiliate:{}", a), EdgeType::ReferredBy));
            }
        }
        "GameOpened" | "GridClick" => {
            if let Some(g) = get("game") {
                edges.push((EntityType::Game, format!("game:{}", g), EdgeType::OpenedGame));
                if let Some(p) = get("gameTrackingProvider") {
                    // Provider linked to game, not member — handled separately below.
                    ops.push(GraphOp::Node {
                        entity_type: EntityType::Game,
                        key: format!("game:{}", g),
                        edges: vec![(EntityType::Provider, format!("provider:{}", p), EdgeType::ProvidedBy)],
                    });
                }
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
            if let Some(c) = get("currency") {
                edges.push((EntityType::Currency, format!("currency:{}", c), EdgeType::UsesCurrency));
            }
        }
        "PopupModule" | "PopUpModule" => {
            let popup_key = get("trackingId").or_else(|| get("popupType"));
            if let Some(pk) = popup_key {
                edges.push((EntityType::Popup, format!("popup:{}", pk), EdgeType::SawPopup));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
        }
        "API Error" => {
            let error_key = match (get("url"), get("statusCode")) {
                (Some(url), Some(code)) => Some(format!("error:{}:{}", code, url)),
                (Some(url), None) => Some(format!("error:{}", url)),
                _ => get("error").map(|e| format!("error:{}", e)),
            };
            if let Some(ek) = error_key {
                edges.push((EntityType::Error, ek, EdgeType::HitError));
            }
            if let Some(p) = get("platform") {
                edges.push((EntityType::Platform, format!("platform:{}", p), EdgeType::PlaysOnPlatform));
            }
        }
        _ => return,
    }

    if !edges.is_empty() {
        ops.push(GraphOp::Node {
            entity_type: EntityType::Member,
            key: member_key,
            edges,
        });
    }
}

/// Replay a graph op into the GraphStore (runs on consumer thread).
pub(crate) fn apply_graph_op(op: &GraphOp, graph: &mut stupid_graph::GraphStore, seg_id: &str) {
    match op {
        GraphOp::Node { entity_type, key, edges } => {
            let source_id = graph.upsert_node(*entity_type, key, &seg_id.to_string());
            for (target_type, target_key, edge_type) in edges {
                let target_id = graph.upsert_node(*target_type, target_key, &seg_id.to_string());
                graph.add_edge(source_id, target_id, *edge_type, &seg_id.to_string());
            }
        }
    }
}

/// Build a graph from multiple segments (single-threaded, streaming).
pub(crate) fn build_graph_multi(data_dir: &Path, segment_ids: &[String]) -> anyhow::Result<(stupid_graph::GraphStore, u64)> {
    info!("Reading {} segments (streaming)...", segment_ids.len());
    let start = std::time::Instant::now();

    let mut graph = stupid_graph::GraphStore::new();
    let mut total_docs: u64 = 0;
    let mut skipped: u64 = 0;

    for (i, seg_id) in segment_ids.iter().enumerate() {
        let reader = match stupid_segment::reader::SegmentReader::open(data_dir, seg_id) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("Skipping segment '{}': {}", seg_id, e);
                skipped += 1;
                continue;
            }
        };

        let mut seg_docs: u64 = 0;
        for doc_result in reader.iter() {
            match doc_result {
                Ok(doc) => {
                    stupid_connector::entity_extract::EntityExtractor::extract(&doc, &mut graph, seg_id);
                    seg_docs += 1;
                }
                Err(e) => {
                    tracing::warn!("Bad document in '{}': {}", seg_id, e);
                }
            }
        }

        total_docs += seg_docs;
        if (i + 1) % 5 == 0 || i + 1 == segment_ids.len() {
            info!(
                "  Progress: {}/{} segments, {} docs total ({:.1}s)",
                i + 1, segment_ids.len(), total_docs, start.elapsed().as_secs_f64()
            );
        }
    }

    info!(
        "Graph built: {} docs from {} segments in {:.1}s ({} skipped)",
        total_docs, segment_ids.len() - skipped as usize, start.elapsed().as_secs_f64(), skipped
    );

    Ok((graph, total_docs))
}
