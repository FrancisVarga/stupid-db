# Compute Engine Overview

## Overview

The compute engine is the heart of stupid-db. It runs continuously in the background, transforming raw event data into actionable knowledge through clustering, graph analysis, pattern detection, and anomaly detection.

## Architecture

```
┌──────────────────────────────────────────────────────────┐
│                   Compute Scheduler                       │
│                                                           │
│  Priority Queue:                                          │
│  ┌─────────────────────────────────────────────────────┐ │
│  │ P0 (realtime): Streaming K-means, entity extraction │ │
│  │ P1 (minutes):  DBSCAN, co-occurrence update         │ │
│  │ P2 (hourly):   PageRank, Louvain, trend detection   │ │
│  │ P3 (daily):    Full recompute, archive, compaction  │ │
│  └─────────────────────────────────────────────────────┘ │
│                                                           │
│  Worker Pool: N threads (configurable, default: num_cpus) │
│                                                           │
│  ┌─────────┐ ┌─────────┐ ┌─────────┐ ┌─────────┐       │
│  │Worker 0 │ │Worker 1 │ │Worker 2 │ │Worker N │       │
│  └─────────┘ └─────────┘ └─────────┘ └─────────┘       │
└──────────────────────────────────────────────────────────┘
```

## Compute Priorities

### P0 — Realtime (on ingest)
Runs synchronously with the ingestion hot path. Must keep pace with ingest throughput.

| Task | Latency Budget | Description |
|------|---------------|-------------|
| Entity extraction | < 100us | Extract known entities from document fields |
| Edge creation | < 50us | Create/update graph edges |
| Embedding generation | < 5ms | Generate vector via ONNX local model |
| Streaming K-means update | < 100us | Update nearest centroid |

### P1 — Near-Realtime (every few minutes)
Runs in background. Operates on recent batches.

| Task | Frequency | Description |
|------|-----------|-------------|
| DBSCAN on recent events | Every 5min | Detect density-based clusters in new data |
| Co-occurrence matrix update | Every 5min | Update game/feature co-occurrence counts |
| Anomaly score refresh | Every 5min | Flag statistical outliers |

### P2 — Periodic (hourly)
Runs on broader data windows.

| Task | Frequency | Description |
|------|-----------|-------------|
| PageRank | Every 1h | Recalculate node importance scores |
| Louvain community detection | Every 1h | Discover member communities |
| Trend detection | Every 1h | Compare current metrics to baselines |
| Cluster quality assessment | Every 1h | Silhouette scores, cluster stability |

### P3 — Background (daily)
Expensive operations that can tolerate delay.

| Task | Frequency | Description |
|------|-----------|-------------|
| Full K-means recompute | Daily | Re-cluster from scratch with optimal K |
| Cross-segment pattern mining | Daily | Find temporal patterns across full window |
| Graph compaction | Daily | Remove weak edges, merge nodes |
| Insight generation | Daily | LLM-powered summary of what changed |

## Compute Task Trait

```rust
trait ComputeTask: Send + Sync {
    /// Human-readable name
    fn name(&self) -> &str;

    /// Priority level
    fn priority(&self) -> Priority;

    /// Estimated duration (for scheduler)
    fn estimated_duration(&self) -> Duration;

    /// Execute the compute task
    fn execute(&self, state: &mut KnowledgeState) -> Result<ComputeResult>;

    /// Whether this task should run now
    fn should_run(&self, last_run: DateTime<Utc>, state: &KnowledgeState) -> bool;
}
```

## Knowledge State

The compute engine writes to a shared `KnowledgeState` that holds all materialized results:

```rust
struct KnowledgeState {
    // Cluster assignments: member → cluster_id
    clusters: HashMap<NodeId, ClusterId>,
    // Cluster centroids and metadata
    cluster_info: HashMap<ClusterId, ClusterInfo>,
    // Community assignments: node → community_id
    communities: HashMap<NodeId, CommunityId>,
    // PageRank scores: node → score
    pagerank: HashMap<NodeId, f64>,
    // Anomaly flags: entity → anomaly_score
    anomalies: HashMap<NodeId, AnomalyScore>,
    // Detected patterns: pattern_id → pattern
    patterns: Vec<TemporalPattern>,
    // Co-occurrence matrices
    cooccurrence: HashMap<(EntityType, EntityType), SparseMatrix>,
    // Trends: metric → trend
    trends: HashMap<String, Trend>,
    // Proactive insights queue
    insights: VecDeque<Insight>,
}
```

## Scheduler Details

See [scheduler.md](./scheduler.md).

## Algorithm Details

- [Clustering](./algorithms/clustering.md) — K-means, DBSCAN, Mini-batch K-means
- [Graph Algorithms](./algorithms/graph-algorithms.md) — PageRank, Louvain, traversal
- [Pattern Detection](./algorithms/pattern-detection.md) — temporal sequences, co-occurrence
- [Anomaly Detection](./algorithms/anomaly-detection.md) — statistical, density-based, behavioral
