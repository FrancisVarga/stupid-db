# Crate Map

## Overview

The project is organized as a Cargo workspace with 12 crates. Each crate has a focused responsibility and well-defined dependencies.

## Crate Descriptions

### `core`
**Shared types and traits used by all other crates.**

- Document model (`Document`, `DocId`, `DocAddress`, `FieldValue`)
- Entity types (`EntityType`, `EdgeType`, `NodeId`, `EdgeId`)
- Configuration structs (`Config`, `SegmentConfig`, `EmbeddingConfig`)
- Common error types (`StupidError`)
- Shared traits (`Timestamped`, `Identifiable`)

Dependencies: `serde`, `chrono`, `uuid`, `thiserror`

### `segment`
**Time-partitioned storage segments.**

Manages the lifecycle of data segments: creation, writing, sealing, reading, and eviction.

- `SegmentWriter` — append documents to active segment
- `SegmentReader` — mmap-based reader for sealed segments
- `SegmentIndex` — doc_id → offset mapping
- `SegmentRotator` — time-based segment rotation
- `SegmentEvictor` — TTL-based cleanup

Dependencies: `core`, `memmap2`, `rmp-serde`

### `vector`
**Vector embedding index and search.**

Per-segment HNSW indices with cross-segment merged search.

- `VectorIndex` — manages per-segment HNSW instances
- `VectorSearch` — cross-segment top-k merge
- `Quantizer` — optional scalar/PQ quantization

Dependencies: `core`, `usearch` or `hnsw_rs`

### `graph`
**In-memory property graph.**

Stores entities and relationships, supports traversal and graph algorithms.

- `GraphStore` — nodes, edges, adjacency lists, indices
- `Traversal` — BFS, DFS, Dijkstra
- `PageRank` — weighted PageRank computation
- `Louvain` — community detection
- `GraphStats` — degree distribution, density, components

Dependencies: `core`

### `ingest`
**Data ingestion from multiple sources.**

Reads parquet files (local/remote), streaming events, and bulk imports.

- `ParquetImporter` — reads Arrow RecordBatch → Documents
- `StreamIngester` — HTTP/WebSocket event receiver
- `FileWatcher` — watches directories for new parquet files
- `RemoteReader` — S3/HTTP range-request parquet reader
- `Normalizer` — field name and value normalization

Dependencies: `core`, `segment`, `connector`, `arrow-rs`, `parquet`, `notify`, `reqwest`, `aws-sdk-s3`

### `connector`
**Hot-path processing: entity extraction and edge creation.**

Runs synchronously on each ingested batch. Extracts entities from documents and creates graph edges.

- `EntityExtractor` — document fields → entity nodes
- `EdgeDeriver` — entities → typed edges
- `FeatureVectorBuilder` — document → member feature vector

Dependencies: `core`, `graph`, `embedder`, `vector`, `compute`

### `embedder`
**Pluggable embedding generation.**

Converts document text representations into vector embeddings.

- `Embedder` trait — common interface
- `OnnxEmbedder` — in-process ONNX Runtime
- `OllamaEmbedder` — Ollama API client
- `OpenAiEmbedder` — OpenAI API client
- `EmbeddingBatcher` — batch processing for throughput
- `EmbeddingCache` — LRU cache for repeated content

Dependencies: `core`, `ort`, `reqwest`

### `compute`
**Continuous compute engine and algorithms.**

Background processing that materializes knowledge from raw data.

- `Scheduler` — priority-based task scheduling
- `KnowledgeState` — materialized results store
- `KMeans` — streaming mini-batch + full recompute
- `Dbscan` — density-based clustering
- `TemporalMiner` — PrefixSpan sequential patterns
- `CoOccurrence` — entity co-occurrence matrices
- `AnomalyDetector` — multi-signal anomaly scoring
- `TrendDetector` — z-score baseline comparison
- `InsightGenerator` — proactive insight creation

Dependencies: `core`, `vector`, `graph`, `segment`

### `catalog`
**Knowledge catalog: what data exists and what's been computed.**

Provides schema awareness, entity counts, and compute summaries for the LLM and dashboard.

- `SchemaRegistry` — event types and their fields
- `EntityCatalog` — entity type counts and samples
- `ComputeCatalog` — cluster info, community info, patterns
- `CatalogSummary` — condensed version for LLM system prompt

Dependencies: `core`, `segment`, `graph`, `compute`

### `query`
**Query planning and execution.**

Translates LLM-generated query plans into multi-store execution.

- `QueryPlan` — structured plan model
- `PlanValidator` — validates plan before execution
- `PlanExecutor` — executes steps across stores, merges results
- `QuerySession` — conversation state, result set references
- `PlanCache` — LRU cache for repeated queries

Dependencies: `core`, `llm`, `catalog`, `segment`, `vector`, `graph`, `compute`

### `llm`
**LLM backend integration.**

Manages communication with LLM providers for query planning and response synthesis.

- `LlmBackend` trait — common interface
- `OpenAiBackend` — GPT-4o structured output
- `AnthropicBackend` — Claude function calling
- `OllamaBackend` — Local model with JSON parsing
- `PromptManager` — system prompt templates
- `Labeler` — generates human-readable labels for clusters/communities

Dependencies: `core`, `reqwest`

### `server`
**HTTP and WebSocket API server. Main binary.**

The entry point that wires everything together and exposes the API.

- `main.rs` — initialization, configuration, lifecycle
- `api::ingest` — POST endpoints for data ingestion
- `api::query` — SSE streaming query endpoint
- `api::catalog` — catalog browse endpoints
- `api::reports` — report CRUD
- `api::system` — health, metrics, status
- `ws::insights` — WebSocket insight feed
- `AppState` — shared state across handlers

Dependencies: `core`, `segment`, `vector`, `graph`, `ingest`, `connector`, `embedder`, `compute`, `catalog`, `query`, `llm`, `axum`, `tokio`, `tower`

## Crate Sizes (Estimated Lines of Code)

| Crate | Est. LOC | Complexity |
|-------|----------|------------|
| core | ~500 | Low |
| segment | ~1,500 | Medium |
| vector | ~800 | Medium |
| graph | ~2,000 | High |
| ingest | ~1,200 | Medium |
| connector | ~800 | Medium |
| embedder | ~1,000 | Medium |
| compute | ~3,000 | High |
| catalog | ~600 | Low |
| query | ~1,500 | High |
| llm | ~1,000 | Medium |
| server | ~1,500 | Medium |
| **Total** | **~15,400** | |
