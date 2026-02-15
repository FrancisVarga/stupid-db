---
name: Performance Guide
description: Performance optimization guide for stupid-db. Profiling Rust code, memory optimization, algorithm complexity, segment I/O, query latency, and dashboard rendering. Use when optimizing performance or investigating bottlenecks.
version: 1.0.0
---

# Performance Guide

## Performance Budget

| Operation | Target | Critical Path? |
|-----------|--------|---------------|
| Ingest batch (1000 docs) | < 300ms | Yes (hot-path) |
| Entity extraction | < 5ms/batch | Yes |
| Edge derivation | < 10ms/batch | Yes |
| Embedding generation | < 200ms/batch | Yes (external API) |
| Vector insert | < 20ms/batch | Yes |
| Segment write | < 50ms/batch | Yes |
| Document scan (1 segment) | < 100ms | Query path |
| Vector search (top-20) | < 50ms | Query path |
| Graph traversal (depth 3) | < 20ms | Query path |
| PageRank (full graph) | < 5s | Background |
| KMeans (full recompute) | < 10s | Background |
| Dashboard render | < 100ms | User-facing |

## Profiling Tools

### Cargo Flamegraph
```bash
cargo install flamegraph
cargo flamegraph --bin stupid-db -- --config config/dev.toml
```
Produces SVG flamegraph showing where time is spent.

### Criterion Benchmarks
```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_pagerank(c: &mut Criterion) {
    let graph = create_test_graph(10_000, 50_000);
    c.bench_function("pagerank_10k", |b| {
        b.iter(|| pagerank(&graph, 0.85, 100, 1e-6))
    });
}

criterion_group!(benches, bench_pagerank);
criterion_main!(benches);
```

### Tracing-Based Profiling
```rust
use tracing::instrument;

#[instrument(skip(data))]
pub fn expensive_operation(data: &[Vec<f64>]) -> Vec<f64> {
    let _span = tracing::info_span!("inner_loop").entered();
    // ...
}
```
With `tracing-timing` or `tracing-flame` subscriber for detailed timing.

## Memory Optimization

### Segment Memory
- **mmap for sealed segments**: OS manages page cache, not heap
- **Sequential reading** (not parallel): Prevents concurrent mmap fault spikes
- **Eviction removes mmaps**: Dropping `Mmap` releases address space

### Graph Memory
The in-memory graph is the largest heap consumer:
- **Node properties**: Use compact representations (enums vs strings)
- **Edge storage**: Consider edge list vs adjacency matrix based on density
- **Segment index**: `HashMap<SegmentId, Vec<EdgeId>>` for efficient eviction

### Embedding Cache
- LRU cache with bounded size
- Key: content hash (not full text)
- Prevents re-computing embeddings for repeated content

### General
- Prefer `Vec<u8>` + manual parsing over `serde_json::Value` for large payloads
- Use `Arc<str>` for frequently shared strings
- Use `SmallVec` for small, known-size collections
- Profile heap with DHAT or heaptrack

## CPU Optimization

### Rayon Parallelism
```rust
use rayon::prelude::*;

// Good: large parallel workload
let scores: Vec<f64> = items.par_iter()
    .map(|item| compute_score(item))
    .collect();

// Bad: small workload (overhead > benefit)
let small_result: Vec<f64> = (0..10).into_par_iter()
    .map(|i| i as f64 * 2.0)
    .collect();
```

**Rule**: Only parallelize when N > 1000 or per-item work > 1ms.

### SIMD-Friendly Data Layout
For vector operations (distance calculations):
- Store vectors as contiguous `Vec<f32>` (SoA layout)
- Align to 32-byte boundaries for AVX
- Use `std::simd` (nightly) or platform intrinsics

### Algorithm Complexity

| Algorithm | Time | Space | Notes |
|-----------|------|-------|-------|
| KMeans | O(n × k × iter) | O(n + k × d) | Mini-batch reduces n |
| DBSCAN | O(n²) naive | O(n) | Use spatial index for O(n log n) |
| PageRank | O(E × iter) | O(V) | E = edges, V = vertices |
| Louvain | O(E × iter) | O(V + E) | Usually few iterations |
| PrefixSpan | O(n × L^d) worst | O(n × L) | L = avg sequence, d = max depth |
| HNSW search | O(log n) | O(n × M) | M = neighbors per node |

## I/O Optimization

### Segment I/O
- Write: Buffered writer (`BufWriter`) with 64KB buffer
- Read: mmap with sequential access hints (`madvise(SEQUENTIAL)`)
- Compression: zstd level 3 (good balance speed/ratio)

### Network I/O
- Embedding API: Batch requests (max batch per provider)
- LLM API: Stream responses for faster time-to-first-token
- S3/Remote: Range requests with parallel downloads for large files

### Dashboard I/O
- SSE streaming: Send render blocks as they're ready (don't buffer all)
- WebSocket: Binary frames for large insight payloads
- D3: Use `requestAnimationFrame` for smooth transitions

## Query Optimization

### Plan Optimization
- Prefer ComputeRead over DocumentScan + Aggregate (pre-computed is faster)
- Add time-range filters to DocumentScan (skip irrelevant segments)
- Limit VectorSearch top_k to minimum needed
- Use Parallel merge strategy for independent steps

### Execution Optimization
- Skip segments outside query time range
- Use segment index for O(1) document lookup
- Cache frequent plan results (PlanCache)
- Stream results as they arrive (don't wait for all steps)

## Monitoring

Key metrics to track:
- Ingestion throughput (docs/sec)
- Hot-path latency (p50, p95, p99)
- Query latency by step type
- Memory usage (RSS, heap, mmap)
- Graph size (nodes, edges)
- Compute task duration
- Embedding cache hit rate
