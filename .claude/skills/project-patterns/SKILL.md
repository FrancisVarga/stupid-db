---
name: Project Patterns
description: Core patterns and conventions for the stupid-db project. Use when working on any part of the codebase, understanding architecture, or making design decisions.
version: 1.0.0
---

# Project Patterns

## Architecture Pattern: Three-Store Model

Every ingested document automatically populates three stores:
1. **Document Store** — Raw events in time-partitioned MessagePack segments
2. **Vector Index** — Per-segment HNSW embeddings for semantic search
3. **Graph Store** — In-memory property graph for entity relationships

## Data Flow Pattern

```
Source → Ingest → Connect (extract + embed + link) → [Doc | Vec | Graph] → Compute → Catalog → Query
```

The "Connect" phase is synchronous hot-path: every batch goes through entity extraction, embedding generation, and graph edge creation before acknowledgment.

## Segment Lifecycle

```
Active (writing) → Sealed (read-only, mmap) → Archived (optional) → Evicted (deleted)
```

- Segments rotate on time boundaries (configurable, default 1 hour)
- TTL-based eviction: segments older than retention window are dropped entirely
- Eviction is O(1): drop segment file + remove per-segment vector index + prune graph edges

## Pluggable Backend Pattern

All external services use trait-based abstraction:
- `Embedder` trait → OnnxEmbedder, OllamaEmbedder, OpenAiEmbedder
- `LlmBackend` trait → OpenAiBackend, AnthropicBackend, OllamaBackend
- Selection via config/env: `EMBEDDING_PROVIDER`, `LLM_PROVIDER`

## Entity Model

Entities extracted from documents: Member, Device, Game, Popup, Error, VipGroup, Affiliate, Currency, Platform, Provider

Join keys: memberCode, fingerprint, gameUid, rGroup, affiliateId, currency, @timestamp

## Crate Dependency Direction

```
core ← segment ← ingest
core ← vector ← compute
core ← graph ← connector
              ← catalog
              ← query ← server
              ← llm
              ← embedder
```

Dependencies flow upward. `core` has zero internal dependencies. `server` depends on everything.

## Anti-Patterns to Avoid

- Never hardcode a single LLM/embedding provider
- Never design append-only storage (must support rolling eviction)
- Never create monolithic single-crate architecture
- Never use Chart.js (always D3.js)
- Never add authentication to the dashboard
- Never modify sample data at D:\w88_data
