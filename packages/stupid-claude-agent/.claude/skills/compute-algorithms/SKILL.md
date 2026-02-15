---
name: Compute Algorithms
description: Compute algorithm patterns for stupid-db. KMeans clustering, DBSCAN, anomaly detection, temporal pattern mining (PrefixSpan), co-occurrence matrices, and trend detection. Use when implementing or tuning algorithms.
version: 1.0.0
---

# Compute Algorithms

## Algorithm Selection Guide

| Question | Best Algorithm | Why |
|----------|---------------|-----|
| "Group similar members" | KMeans (known k) or DBSCAN (unknown k) | Clustering on feature vectors |
| "Find outlier members" | DBSCAN (noise points) or Anomaly Detection | Outliers as non-clustered or high-score |
| "Common behavior sequences" | PrefixSpan | Sequential pattern mining |
| "Which entities appear together" | Co-occurrence Matrix | Pairwise frequency counting |
| "Is this metric unusual" | Trend Detection (z-score) | Statistical baseline comparison |
| "Multi-signal alerting" | Anomaly Detection | Combines statistical + behavioral + graph |
| "Most influential entities" | PageRank (see graph-algorithms) | Graph-based importance |
| "Natural entity groups" | Louvain (see graph-algorithms) | Community structure detection |

## KMeans Clustering

### Streaming Mini-Batch KMeans
```rust
pub struct StreamingKMeans {
    k: usize,
    centroids: Vec<Vec<f64>>,
    batch_size: usize,
    max_iterations: usize,
    convergence_threshold: f64,
}
```

**Two modes**:
1. **Mini-batch** (streaming): Process small batches as data arrives. Fast, approximate.
2. **Full recompute** (periodic): Recompute from all vectors. Slower, more accurate.

**Input**: Vector embeddings from the vector index
**Output**: `ClusterResult { centroids, assignments, silhouette_score }`

### Parameter Tuning
- `k`: Start with sqrt(N/2), refine with silhouette score
- `batch_size`: 100-1000 for mini-batch (larger = more stable, slower)
- `convergence_threshold`: 1e-4 for production, 1e-6 for quality

## DBSCAN

```rust
pub struct Dbscan {
    eps: f64,          // neighborhood radius
    min_points: usize, // minimum density
}
```

**Advantages over KMeans**:
- No need to specify k
- Finds arbitrarily shaped clusters
- Identifies outliers (noise points)

**Input**: Vector embeddings with distance metric (cosine or euclidean)
**Output**: `DbscanResult { clusters: Vec<Vec<usize>>, noise: Vec<usize> }`

### Parameter Tuning
- `eps`: Use k-distance graph to find "elbow". Start with 0.5 for cosine distance.
- `min_points`: Generally 2 × dimensions. For 384-dim embeddings, start with 5-10.
- Warning: DBSCAN is O(n²) without spatial index. Use approximate methods for large datasets.

## Anomaly Detection

### Multi-Signal Scoring
```rust
pub struct AnomalyDetector {
    statistical_weight: f64,  // 0.3
    behavioral_weight: f64,   // 0.3
    graph_weight: f64,        // 0.2
    temporal_weight: f64,     // 0.2
}
```

**Signals**:

1. **Statistical**: Z-score deviation from rolling mean/std
   - Metric: event counts, error rates, session durations
   - Alert when |z| > 2.5

2. **Behavioral**: Change in entity interaction patterns
   - Metric: game diversity, device switching, activity hours
   - Compare recent window vs historical baseline

3. **Graph**: Unusual connectivity patterns
   - Metric: degree change, new community membership, centrality shift
   - Alert on sudden graph structure changes

4. **Temporal**: Sequence anomaly vs historical patterns
   - Metric: event sequence probability from PrefixSpan
   - Alert on rare sequences

**Combined score**: Weighted sum, normalized to [0, 1]. Higher = more anomalous.

## Temporal Pattern Mining (PrefixSpan)

```rust
pub struct PrefixSpan {
    min_support: f64,        // minimum frequency (0.0-1.0)
    max_pattern_length: usize, // max sequence length
    time_window: Duration,    // events within this window form a sequence
}
```

**Input**: Event sequences per entity (ordered by timestamp)
**Output**: `Vec<SequentialPattern { sequence, support, confidence }>`

### Example Patterns
```
Login → GameOpened → GameOpened → Logout  (support: 0.45)
Login → PopupSeen → GameOpened            (support: 0.32)
Error → Error → Logout                   (support: 0.12)
```

### Parameter Tuning
- `min_support`: 0.05-0.1 for discovery, 0.2+ for common patterns
- `max_pattern_length`: 3-5 for performance, longer for detailed sequences
- `time_window`: 1 hour for session patterns, 1 day for daily patterns
- Warning: Low support + long patterns = combinatorial explosion

## Co-occurrence Matrix

```rust
pub struct CoOccurrence {
    entity_type_a: EntityType,
    entity_type_b: EntityType,
    time_window: Duration,
    matrix: HashMap<(String, String), u64>, // (entity_a, entity_b) → count
}
```

**Use cases**:
- Game × Game: "Members who played X also played Y"
- Member × Error: "Which members see which errors"
- Game × Platform: "Platform distribution per game"

### Visualization
The co-occurrence matrix maps directly to the dashboard's `CooccurrenceHeatmap.tsx` component.

## Trend Detection (Z-Score)

```rust
pub struct TrendDetector {
    window_size: usize,     // rolling window for baseline
    alert_threshold: f64,   // z-score threshold (default 2.5)
}
```

**Process**:
1. Compute rolling mean and std over window
2. For each new observation, calculate z-score
3. Alert when |z-score| > threshold

**Metrics to track**: Event counts per type, error rates, unique member counts, average session duration.

## Scheduler Integration

All algorithms register as `ComputeTask`:

```rust
impl ComputeTask for KMeansTask {
    fn name(&self) -> &str { "kmeans_clustering" }
    fn priority(&self) -> u32 { 50 } // medium priority
    fn should_run(&self, state: &KnowledgeState) -> bool {
        state.last_kmeans_run.elapsed() > Duration::from_secs(300)
    }
    fn execute(&self, ctx: &ComputeContext) -> Result<()> {
        // Run algorithm, update KnowledgeState
    }
}
```

Priority levels:
- 100: Critical (anomaly detection — real-time alerting)
- 75: High (trend detection — timely insights)
- 50: Medium (clustering, PageRank — periodic refresh)
- 25: Low (pattern mining, co-occurrence — background discovery)
