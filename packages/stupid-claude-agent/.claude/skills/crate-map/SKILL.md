---
name: Crate Map
description: Complete map of all 12 Cargo workspace crates, their responsibilities, dependencies, key types, and file locations. Use when navigating the codebase, understanding crate boundaries, or adding new code.
version: 1.0.0
---

# Crate Map

## Dependency Graph

```
server ──→ query ──→ llm
  │          │ ──→ catalog ──→ segment ──→ core
  │          │ ──→ segment      │ ──→ graph ──→ core
  │          │ ──→ vector       │ ──→ compute ──→ core
  │          │ ──→ graph
  │          │ ──→ compute
  │
  ├──→ ingest ──→ segment
  │      │ ──→ connector ──→ graph
  │      │                  ──→ embedder ──→ core
  │      │                  ──→ vector
  │      │                  ──→ compute
  │
  ├──→ catalog
  ├──→ compute ──→ vector ──→ core
  │             ──→ graph
  │             ──→ segment
  ├──→ storage
  ├──→ athena
  └──→ queue
```

Dependencies flow upward. `core` has zero internal deps. `server` depends on everything.

## Crate Reference

### `core` — Shared Foundation
**Path**: `crates/core/src/`
**Key types**: `Document`, `DocId`, `FieldValue`, `EntityType`, `EdgeType`, `NodeId`, `Config`, `StupidError`
**Dependencies**: serde, chrono, uuid, thiserror
**Complexity**: Low (~500 LOC)

### `segment` — Time-Partitioned Storage
**Path**: `crates/segment/src/`
**Key types**: `SegmentWriter`, `SegmentReader`, `SegmentIndex`, `SegmentRotator`, `SegmentEvictor`
**Dependencies**: core, memmap2, rmp-serde, zstd
**Complexity**: Medium (~1,500 LOC)

### `vector` — Embedding Index
**Path**: `crates/vector/src/` (planned)
**Key types**: `VectorIndex`, `VectorSearch`, `Quantizer`
**Dependencies**: core, usearch/hnsw_rs
**Complexity**: Medium (~800 LOC)

### `graph` — Property Graph
**Path**: `crates/graph/src/`
**Key types**: `GraphStore`, `Traversal`, `PageRank`, `Louvain`, `GraphStats`
**Dependencies**: core
**Complexity**: High (~2,000 LOC)

### `ingest` — Data Ingestion
**Path**: `crates/ingest/src/`
**Key types**: `ParquetImporter`, `StreamIngester`, `FileWatcher`, `RemoteReader`, `Normalizer`
**Dependencies**: core, segment, connector, arrow, parquet, notify, reqwest, object_store
**Complexity**: Medium (~1,200 LOC)

### `connector` — Hot-Path Processing
**Path**: `crates/connector/src/`
**Key types**: `EntityExtractor`, `EdgeDeriver`, `FeatureVectorBuilder`
**Dependencies**: core, graph, embedder, vector, compute
**Complexity**: Medium (~800 LOC)

### `embedder` — Embedding Generation
**Path**: `crates/embedder/src/` (planned, currently in ingest/embedding/)
**Key types**: `Embedder` trait, `OnnxEmbedder`, `OllamaEmbedder`, `OpenAiEmbedder`
**Dependencies**: core, ort, reqwest
**Complexity**: Medium (~1,000 LOC)

### `compute` — Continuous Compute
**Path**: `crates/compute/src/`
**Key types**: `Scheduler`, `KnowledgeState`, `ComputeTask` trait, algorithm implementations
**Dependencies**: core, vector, graph, segment
**Complexity**: High (~3,000 LOC)

### `catalog` — Knowledge Catalog
**Path**: `crates/catalog/src/`
**Key types**: `SchemaRegistry`, `EntityCatalog`, `ComputeCatalog`, `CatalogSummary`
**Dependencies**: core, segment, graph, compute
**Complexity**: Low (~600 LOC)

### `query` — Query Planning
**Path**: `crates/query/src/`
**Key types**: `QueryPlan`, `QueryStep`, `PlanValidator`, `PlanExecutor`, `QuerySession`
**Dependencies**: core, llm, catalog, segment, vector, graph, compute
**Complexity**: High (~1,500 LOC)

### `llm` — LLM Integration
**Path**: `crates/llm/src/`
**Key types**: `LlmBackend` trait, `OpenAiBackend`, `AnthropicBackend`, `OllamaBackend`, `PromptManager`, `Labeler`
**Dependencies**: core, reqwest
**Complexity**: Medium (~1,000 LOC)

### `server` — HTTP/WS API
**Path**: `crates/server/src/`
**Key types**: `AppState`, API handler modules
**Dependencies**: everything, axum, tokio, tower
**Complexity**: Medium (~1,500 LOC)

### `storage` — Abstract Storage
**Path**: `crates/storage/src/`
**Complexity**: Medium

### `athena` — AWS Athena
**Path**: `crates/athena/src/`
**Complexity**: Medium

### `queue` — Internal Queue
**Path**: `crates/queue/src/`
**Complexity**: Low

## Where to Add New Code

| New Feature | Target Crate | Rationale |
|-------------|-------------|-----------|
| New entity type | `core` (type) + `connector` (extraction) | Entity types in core, extraction rules in connector |
| New algorithm | `compute` | All algorithms live in compute/algorithms/ |
| New LLM provider | `llm` | Implement LlmBackend trait |
| New embedding provider | `embedder` | Implement Embedder trait |
| New API endpoint | `server` | Add handler in api/ module |
| New visualization | `dashboard/components/viz/` | Add D3 component |
| New data source | `ingest` | Add reader implementation |
| New graph algorithm | `graph` | Algorithm in graph, task in compute/scheduler/tasks/ |
