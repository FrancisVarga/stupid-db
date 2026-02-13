# Compute Pipeline

## Overview

The compute pipeline describes how data flows through the continuous compute engine, from raw ingest to materialized knowledge.

## Pipeline Stages

```
Stage 1: Ingest
  │  Raw parquet/stream → Document store
  │
  ▼
Stage 2: Hot Connect (P0, synchronous)
  │  Document → Entities → Graph edges
  │  Document → Embedding → Vector index
  │  Document → Feature vector → Streaming K-means
  │
  ▼
Stage 3: Warm Compute (P1, async background)
  │  Recent events → DBSCAN clustering
  │  Recent events → Co-occurrence matrix update
  │  Recent events → Anomaly scoring
  │
  ▼
Stage 4: Periodic Compute (P2, scheduled)
  │  Full graph → PageRank
  │  Full graph → Louvain community detection
  │  All clusters → Trend detection
  │  All metrics → Baseline comparison
  │
  ▼
Stage 5: Deep Compute (P3, daily)
  │  All vectors → Full K-means recompute
  │  All events → Temporal pattern mining
  │  All results → LLM insight generation
  │
  ▼
Knowledge State (materialized, queryable)
```

## Stage 2: Hot Connect — Detail

This is the most latency-sensitive stage. It runs for every ingested batch.

```rust
fn hot_connect(batch: &[Document], state: &mut KnowledgeState) {
    for doc in batch {
        // 1. Entity extraction (< 100us)
        let entities = extract_entities(doc);

        // 2. Graph updates (< 50us per edge)
        for (source, target, edge_type) in derive_edges(&entities) {
            state.graph.add_edge(source, target, edge_type, doc.segment_id);
        }

        // 3. Embedding (< 5ms, batched for throughput)
        let embedding = embedder.embed(&doc.to_text_repr());
        state.vector_index.insert(doc.id, embedding);

        // 4. Streaming K-means (< 100us)
        let features = extract_feature_vector(doc);
        state.streaming_kmeans.update(doc.member_id(), features);
    }
}
```

### Entity Extraction Rules

Hardcoded extraction based on event type and known fields:

```rust
fn extract_entities(doc: &Document) -> Vec<Entity> {
    let mut entities = vec![];

    // Always extract these if present
    if let Some(mc) = doc.get("memberCode") {
        entities.push(Entity::Member(mc.to_string()));
    }
    if let Some(fp) = doc.get("fingerprint") {
        entities.push(Entity::Device(fp.to_string()));
    }
    if let Some(curr) = doc.get("currency") {
        entities.push(Entity::Currency(curr.to_string()));
    }
    if let Some(rg) = doc.get("rGroup") {
        entities.push(Entity::VipGroup(rg.to_string()));
    }

    // Event-type specific
    match doc.event_type.as_str() {
        "GameOpened" => {
            if let Some(game) = doc.get("gameUid") {
                entities.push(Entity::Game(game.to_string()));
            }
        }
        "API Error" => {
            if let Some(err) = doc.get("error") {
                entities.push(Entity::Error(err.to_string()));
            }
        }
        _ => {}
    }

    entities
}
```

### Feature Vector Construction

For member-level clustering, we build feature vectors from aggregated behavior:

```
Member feature vector (updated incrementally):
[
  login_count_7d,          // How often they login
  game_count_7d,           // How many games opened
  unique_games_7d,         // Variety of games
  error_count_7d,          // Error frequency
  popup_interaction_7d,    // Popup engagement
  platform_mobile_ratio,   // Mobile vs desktop
  session_count_7d,        // Number of sessions
  avg_session_gap_hours,   // Time between sessions
  vip_group_numeric,       // VIP tier encoded as number
  currency_encoded,        // Currency one-hot or encoded
]
```

## Stage 3: Warm Compute — Detail

Runs every 5 minutes on accumulated recent data.

```rust
fn warm_compute(state: &mut KnowledgeState) {
    // DBSCAN on recent event embeddings
    let recent_vectors = state.vector_index.get_since(5.minutes.ago());
    let dbscan_result = dbscan(recent_vectors, eps=0.3, min_pts=5);
    state.merge_dbscan_results(dbscan_result);

    // Co-occurrence update
    let recent_events = state.document_store.scan_since(5.minutes.ago());
    state.cooccurrence.update(recent_events);

    // Anomaly scoring
    for member in state.recently_active_members() {
        let score = compute_anomaly_score(member, state);
        if score > ANOMALY_THRESHOLD {
            state.anomalies.insert(member, score);
            state.insights.push(Insight::Anomaly(member, score));
        }
    }
}
```

## Pipeline Metrics

Each stage reports throughput and latency:

```rust
struct PipelineMetrics {
    // Stage 2 (hot)
    hot_docs_per_second: f64,
    hot_avg_latency_us: f64,
    hot_embedding_avg_ms: f64,

    // Stage 3 (warm)
    warm_last_run: DateTime<Utc>,
    warm_duration_ms: u64,
    warm_events_processed: u64,

    // Stage 4 (periodic)
    periodic_last_run: HashMap<String, DateTime<Utc>>,

    // Stage 5 (deep)
    deep_last_run: DateTime<Utc>,
    deep_duration_seconds: u64,
}
```
