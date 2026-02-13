# stupid-db Documentation Index

## Architecture
- [Overview](./architecture/overview.md) — System boundaries, design principles, three-store model
- [Data Flow](./architecture/data-flow.md) — End-to-end pipeline from source to queryable knowledge

### Storage
- [Segment Model](./architecture/storage/segment-model.md) — Time-partitioned segments, lifecycle, eviction
- [Document Store](./architecture/storage/document-store.md) — Primary write interface, MessagePack storage, schema registry
- [Vector Index](./architecture/storage/vector-index.md) — Per-segment HNSW, embedding models, cross-segment search
- [Graph Store](./architecture/storage/graph-store.md) — In-memory property graph, nodes, edges, traversal
- [Remote Data](./architecture/storage/remote-data.md) — S3/HTTP parquet access, DuckDB-style range requests
- [AWS Integration](./architecture/storage/aws-integration.md) — Athena, Aurora, RDS connectivity and enrichment

### Architecture Decision Records
- [ADR-001: Rust](./architecture/decisions/adr-001-language-rust.md) — Why Rust over Go
- [ADR-002: Segments](./architecture/decisions/adr-002-segment-storage.md) — Why time-partitioned segments
- [ADR-003: Continuous Compute](./architecture/decisions/adr-003-continuous-compute.md) — Why background compute over on-demand
- [ADR-004: LLM Query](./architecture/decisions/adr-004-llm-query-interface.md) — Why natural language over SQL/DSL

## Ingestion
- [Overview](./ingestion/overview.md) — Sources, formats, API, throughput
- [Data Profile](./ingestion/data-profile.md) — Sample dataset analysis (event types, schemas, volumes)
- [Entity Extraction](./ingestion/entity-extraction.md) — Document fields → graph nodes and edges
- [Embedding](./ingestion/embedding.md) — Text representation, model backends, batching
- [Connector (Hot Path)](./ingestion/connector.md) — Synchronous pipeline: extract, embed, connect

## Compute
- [Overview](./compute/overview.md) — Compute engine architecture, priorities, knowledge state
- [Pipeline](./compute/pipeline.md) — Stage-by-stage processing detail
- [Scheduler](./compute/scheduler.md) — Priority-based scheduling, backpressure, monitoring
- [Eviction](./compute/eviction.md) — Segment expiry, graph cleanup, computed result archival

### Algorithms
- [Clustering](./compute/algorithms/clustering.md) — K-Means (streaming + full), DBSCAN
- [Graph Algorithms](./compute/algorithms/graph-algorithms.md) — PageRank, Louvain community detection, shortest path
- [Pattern Detection](./compute/algorithms/pattern-detection.md) — Temporal sequences (PrefixSpan), co-occurrence, trends
- [Anomaly Detection](./compute/algorithms/anomaly-detection.md) — Multi-signal scoring: statistical, DBSCAN noise, behavioral, graph

## Query
- [Overview](./query/overview.md) — Natural language → query plan → execution → response
- [Query Plan](./query/query-plan.md) — Plan structure, step types, execution engine, validation
- [Catalog](./query/catalog.md) — Schema/entity/compute catalogs, LLM context, browse API
- [LLM Integration](./query/llm-integration.md) — OpenAI/Claude/Ollama backends, prompts, streaming

## Dashboard
- [Overview](./dashboard/overview.md) — Next.js + D3.js, chat-first design, tech stack
- [Chat Interface](./dashboard/chat-interface.md) — Layout, streaming UX, follow-ups, quick commands

### Components
- [Visualizations](./dashboard/components/visualizations.md) — D3 components: bar, line, scatter, force graph, sankey, heatmap, treemap, table
- [Proactive Insights](./dashboard/components/insights.md) — WebSocket feed, insight types, sidebar UI
- [Reports](./dashboard/components/reports.md) — Saved conversations, export (CSV/PNG/PDF), permalinks

## Project
- [Structure](./project/structure.md) — Full repository layout (Rust workspace + Next.js dashboard)
- [Tech Stack](./project/tech-stack.md) — All crates, packages, tools, system requirements
- [Crate Map](./project/crate-map.md) — 12 Rust crates: responsibilities, dependencies, estimated LOC

## Data
- [Sample Profile](./data/sample-profile.md) — D:\w88_data analysis: schemas, volumes, quality issues, join keys
