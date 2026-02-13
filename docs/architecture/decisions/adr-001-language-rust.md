# ADR-001: Rust as Implementation Language

## Status
**Accepted**

## Context

The system processes 3-5TB of data over a 30-day rolling window with continuous compute. Language choice affects memory efficiency, concurrency model, embedding integration, and operational overhead.

Candidates: **Rust** and **Go**.

## Decision

**Rust** is the implementation language for the core engine.

## Rationale

### Memory Efficiency
- 3-5TB data scale means memory management matters
- No garbage collector — no GC pauses during compute-heavy operations
- Mmap integration is first-class (no GC interference with mmap'd pages)
- Arena allocators for graph structures — predictable allocation patterns

### Embedding Integration
- ONNX Runtime has excellent Rust bindings (`ort` crate)
- Can run embedding models in-process — no sidecar, no network hop
- `candle` (Hugging Face's Rust ML framework) as alternative
- Go's ML story is weaker — typically requires CGo bridges

### Concurrency
- Async runtime (tokio) for I/O-bound work (network, remote reads)
- Rayon for CPU-bound work (compute algorithms, batch processing)
- Zero-cost abstractions for parallelism
- Type system prevents data races at compile time

### Ecosystem
- `tantivy` — full-text search (if needed)
- `hora`, `usearch`, `hnsw` — vector index implementations
- `arrow-rs` — native Arrow/Parquet support
- `axum` — high-performance HTTP server
- `rmp-serde` — MessagePack serialization
- `duckdb-rs` — DuckDB bindings (potential integration)

### Why Not Go
- GC pauses problematic at this data scale
- Weaker ML/embedding ecosystem
- Interface-based generics less expressive for trait-based plugin systems
- Memory overhead of goroutine stacks adds up with millions of entities

## Consequences

- Steeper learning curve for contributors
- Longer compilation times
- More verbose error handling
- Excellent runtime performance and memory efficiency
- Strong safety guarantees

## Frontend Exception

The dashboard uses **Next.js + TypeScript** — a separate process communicating with the Rust backend via HTTP/WebSocket. This is standard practice and plays to each language's strengths.
