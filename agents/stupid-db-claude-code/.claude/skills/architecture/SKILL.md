---
name: architecture
description: Three-store knowledge materialization architecture patterns for stupid-db
triggers:
  - architecture
  - data flow
  - three stores
  - system design
  - crate dependency
---

# Architecture Patterns

## Three-Store Model

stupid-db unifies three database paradigms behind a single write interface:

| Store | Purpose | Backing | Hot Path |
|-------|---------|---------|----------|
| Document | Raw event storage, filtered scans | Time-partitioned mmap segments (MessagePack) | SegmentReader scan with predicate push-down |
| Vector | Semantic similarity, clustering input | Per-segment HNSW index | Cosine similarity search |
| Graph | Entity relationships, traversals | In-memory property graph (adjacency list) | PageRank, Louvain, degree queries |

All three stores are populated automatically from a single document insert. Users never interact with vector or graph stores directly.

## Data Flow Pipeline

```
Source (Parquet/S3/SQS/Athena)
  → Ingest (parse, validate)
  → Connect (entity extract + embed + link)
  → [Document Store | Vector Index | Graph Store]
  → Compute (continuous background algorithms)
  → Catalog (queryable metadata)
  → Query (LLM-powered NL → SQL)
  → Dashboard (D3.js visualization)
```

## Crate Dependency Rules

Dependencies flow upward — core has zero internal deps:

```
Layer 0: core (types, Config, Document)
Layer 1: rules, segment, graph, llm
Layer 2: compute, connector, ingest, storage, catalog
Layer 3: server, tool-runtime, agent
Layer 4: mcp, cli
Standalone: notify, queue, athena
```

**Critical rule**: No dependency cycles. If crate A depends on B, B must NEVER depend on A.

## Segment Lifecycle

```
Active (writing) → Sealed (read-only) → Archived (cold) → Evicted (dropped)
```

- 15-30 day rolling window with TTL-based eviction
- Eviction is O(1) — just drop the segment directory
- Never design for append-only storage

## Design Principles

1. **Single write interface** — insert document, get all three representations free
2. **Continuous compute** — algorithms run perpetually in background, not on-demand
3. **Time-partitioned segments** — data has TTL, eviction is O(1) segment drop
4. **Compute over storage** — optimize for processing speed, not persistence durability
5. **LLM-native query** — natural language in, structured reports + visualizations out
6. **Remote-capable** — read parquet from S3/HTTP like DuckDB's httpfs
7. **AWS-integrated** — query Athena, read from Aurora/RDS as enrichment
