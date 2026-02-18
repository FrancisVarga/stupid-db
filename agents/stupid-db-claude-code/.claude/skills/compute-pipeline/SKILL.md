---
name: compute-pipeline
description: Continuous background compute algorithms including clustering, graph analysis, anomaly detection, and pattern mining
triggers:
  - compute
  - pipeline
  - algorithm
  - anomaly detection
  - clustering
  - trend detection
  - pattern mining
  - PrefixSpan
  - PageRank
  - Louvain
---

# Compute Pipeline

## Overview

The compute crate runs algorithms continuously in the background, not on-demand. Results are materialized into queryable state via the `KnowledgeState`.

## Key Types

```rust
pub use engine::ComputeEngine;
pub use pipeline::Pipeline;
pub use pipeline::metrics::PipelineMetrics;
pub use scheduler::{Scheduler, SchedulerConfig, ComputeTask, Priority};
pub use scheduler::types::{AnomalyClassification, AnomalyResult};
```

## Algorithms

### Clustering
- **K-Means** — Streaming + batch modes for entity grouping
- **DBSCAN** — Density-based clustering for noise detection

### Graph Analysis
- **PageRank** — Entity importance scoring
- **Louvain** — Community detection in entity graph
- **Degree centrality** — Connection count analysis

### Pattern Detection
- **PrefixSpan** — Temporal sequence mining (compressed event types)
- **Co-occurrence** — Entity co-occurrence matrix
- **Trend detection** — Magnitude + direction (up/down/flat) with severity

### Anomaly Detection
- **Multi-signal scoring** — Combines statistical, behavioral, and graph signals
- **Statistical outlier** — Z-score based
- **Behavioral drift** — Cosine distance from baseline
- **Graph anomaly** — Structural anomalies in entity relationships

## Pipeline Modules

```
crates/compute/src/pipeline/
├── mod.rs          # Pipeline orchestration
├── features.rs     # 10-dimensional feature extraction
├── cooccurrence.rs # Entity co-occurrence matrix
├── trend.rs        # TrendDetector with Severity enum
├── anomaly.rs      # Multi-signal anomaly scoring
└── metrics.rs      # PipelineMetrics tracking
```

## Scheduler

The `Scheduler` manages compute task priorities:

```rust
pub enum Priority {
    Critical,  // Anomaly detection on fresh data
    High,      // Feature recomputation
    Normal,    // Background clustering
    Low,       // Pattern mining, archival
}
```

## Integration Pattern

When adding a new compute algorithm:
1. Implement in `crates/compute/src/algorithms/`
2. Add pipeline stage in `crates/compute/src/pipeline/`
3. Add task variant to `ComputeTask` enum
4. Register with `Scheduler` at appropriate priority
5. Add config kind to rules system if user-configurable
6. Expose results via server API endpoint
