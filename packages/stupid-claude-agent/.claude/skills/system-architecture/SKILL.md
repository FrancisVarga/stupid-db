---
name: System Architecture
description: Full system architecture for stupid-db. Three-store model, data flow, segment lifecycle, design principles, and system boundaries. Use when making architecture decisions, understanding data flow, or reviewing system design.
version: 1.0.0
---

# System Architecture

## Core Thesis

Raw event logs are fuel, not the product. The product is **computed knowledge** that emerges from continuously processing, connecting, and analyzing events across document, vector, and graph stores simultaneously.

## Three-Store Model

Every ingested document automatically populates three stores:

| Store | Purpose | Backing | Query Style |
|-------|---------|---------|-------------|
| **Document** | Raw event storage, filtered scans, projections | Time-partitioned mmap segments (MessagePack + zstd) | Filter + project + aggregate |
| **Vector** | Semantic similarity, embedding-based search | Per-segment HNSW index | Top-k nearest neighbor |
| **Graph** | Entity relationships, traversals, community structure | In-memory property graph (adjacency list) | Traversal + algorithms |

Users never interact with vector or graph stores directly — they emerge automatically from the ingestion pipeline.

## Data Flow

```
Source → Ingest → Connect (extract + embed + link) → [Doc | Vec | Graph] → Compute → Catalog → Query
```

### Phases

1. **Ingest**: Read Parquet/stream → normalize → batch
2. **Connect** (hot-path, synchronous): Entity extraction → edge derivation → embedding → vector insert
3. **Store**: Document written to segment, vectors to HNSW, entities/edges to graph
4. **Compute** (continuous background): Algorithms run perpetually — clustering, PageRank, anomaly detection
5. **Catalog**: Schema registry + entity catalog + compute catalog updated
6. **Query**: LLM generates QueryPlan → executor runs against stores → synthesize response

## Segment Lifecycle

```
Active (writing) → Sealed (read-only, mmap) → Archived (optional) → Evicted (deleted)
```

- Segments rotate on time boundaries (default 1 hour)
- TTL-based eviction: segments older than retention window (15-30 days) are dropped
- Eviction is O(1): drop segment file + remove per-segment vector index + prune graph edges
- Never design append-only — everything expires

## Design Principles

1. **Single write interface** — insert a document, get all three representations for free
2. **Continuous compute** — algorithms run perpetually in background, not on-demand
3. **Time-partitioned segments** — data has 15-30 day TTL, eviction is O(1)
4. **Compute over storage** — optimize for processing speed, not persistence durability
5. **LLM-native query** — natural language in, structured reports + visualizations out
6. **Remote-capable** — read parquet from S3/HTTP like DuckDB's httpfs
7. **AWS-integrated** — query Athena, read from Aurora/RDS as enrichment

## Pluggable Backend Pattern

All external services use trait-based abstraction:
- `Embedder` trait → OnnxEmbedder, OllamaEmbedder, OpenAiEmbedder
- `LlmBackend` trait → OpenAiBackend, AnthropicBackend, OllamaBackend
- Selection via env: `EMBEDDING_PROVIDER`, `LLM_PROVIDER`

Never hardcode a single provider.

## Anti-Patterns

- Append-only storage (must support rolling eviction)
- Monolithic single-crate architecture (use 12-crate workspace)
- Single LLM/embedding provider (must be pluggable)
- Traditional BI dashboards (chat-first interface only)
- Batch ETL (streaming materialization / Kappa architecture)
- SQL generation (LLM generates structured QueryPlans)
