# stupid-db — Claude Code Project Instructions

## What is stupid-db?

A **continuous knowledge materialization engine** that unifies document, vector, and graph databases behind a single ingestion interface. Raw event data (parquet, streams, SQS, Athena) flows in and gets continuously transformed into queryable knowledge through background compute algorithms.

**Core thesis**: Raw event logs are fuel, not the product. The product is computed knowledge.

## Architecture (18 Cargo Workspace Crates)

```
Sources: Parquet | S3/HTTP | SQS Queues | Athena/Aurora
    │
    ▼
ingest → connector → [Doc|Vec|Graph] → compute
storage (Local/S3) ← segment (mmap) ← core (types)
catalog ← rules (YAML) ← graph (PageRank/Louvain)
llm (OpenAI/Claude/Ollama) → server (Axum HTTP/SSE)
tool-runtime → mcp → cli → agent (team exec)
notify | queue | athena
    │
    ▼
dashboard/ — Next.js 16 + D3.js + AI SDK v6
Chat-first interface, no auth, internal network only
```

### Crate Dependency Flow (no cycles allowed)
core → rules → {compute, connector, ingest} → server
core → segment → storage
core → graph
core → llm → tool-runtime → {mcp, cli}
core → agent
core → {notify, queue, athena, catalog}

### Crate Purposes

| Crate | Purpose |
|-------|---------|
| **core** | Shared types, Config, Document model, traits — zero internal deps |
| **rules** | YAML rule loader with 6 kinds, two-pass deser, extends/deep-merge |
| **compute** | Algorithms (KMeans, DBSCAN, PrefixSpan), anomaly detection, trends |
| **connector** | Entity extraction from documents, graph edge derivation |
| **ingest** | Parquet reading, embedding generation, streaming events |
| **segment** | SegmentWriter/Reader, mmap MessagePack, rotation, eviction |
| **storage** | StorageEngine abstraction (Local/S3), SegmentCache |
| **graph** | In-memory property graph, PageRank, Louvain communities |
| **catalog** | Schema registry, QueryExecutor, QueryPlan |
| **llm** | Multi-provider LLM (OpenAI/Claude/Ollama), QueryGenerator |
| **tool-runtime** | AgenticLoop, Tool trait, ToolRegistry, permissions, streaming |
| **mcp** | MCP JSON-RPC server/client over stdio/channels |
| **cli** | Interactive CLI for agentic conversations |
| **agent** | AgentExecutor, TeamExecutor, YAML agent configs |
| **server** | Axum HTTP/WebSocket/SSE API — ties everything together |
| **notify** | Notification system |
| **queue** | SQS queue consumer with configurable polling |
| **athena** | AWS Athena connector with SSE query streaming |

## Behavioral Rules (MUST Follow)

### Code Quality
- ALWAYS read a file before editing it
- ALWAYS prefer editing existing files over creating new ones
- NEVER create monolithic single-crate architecture — use workspace crates
- NEVER commit secrets, credentials, or .env files
- NEVER save working files or tests to the root folder

### Rust Conventions
- Use `cargo nextest run` instead of `cargo test` (90% faster)
- Use rust-analyzer for validation, not `cargo check`
- Error handling: `thiserror` for libraries, `anyhow` for binaries
- Logging: Always `tracing`, never `println!`
- Async: `tokio` for I/O, `rayon` for CPU-bound work
- Use `pub(crate)` for cross-store shared helpers — never duplicate code
- Place `rustc-wrapper` in `[build]` section of `.cargo/config.toml`

### Three-Tier Credential System
Config (internal) → Safe (API/masked) → Credentials (consumer)
Never expose raw credentials in API responses. Use the Safe variant with masked passwords.

### LLM Provider Rules
- Query interface supports OpenAI, Claude, and Ollama — NEVER hardcode single provider
- Use trait-based LlmProvider / SimpleLlmProvider abstractions
- The LlmProviderBridge adapts non-streaming to streaming interfaces

### Storage Rules
- Segment storage: 15-30 day rolling window with TTL eviction
- Design for continuous eviction, NOT append-only
- Segment lifecycle: Active → Sealed → Archived → Evicted
- Three stores populated from single insert: Document + Vector + Graph

### Dashboard Rules
- Chat-first interface — NOT traditional BI panels with dropdowns/filters
- Use D3.js for ALL visualizations — NEVER Chart.js or other libraries
- No authentication — assume internal/trusted network deployment
- Use refreshKey pattern for parent-child component sync
- 4-layer data flow: Form → Next.js proxy → Rust backend → Encrypted JSON

### Data Safety
- D:\w88_data is read-only production sample data — NEVER modify
- Sample parquet files define entity extraction schema — analyze before changing entity model

## Rules System (6 Kinds)

Two-pass YAML deserialization: RuleEnvelope (header) → RuleDocument (kind-specific).

| Kind | Purpose | Compiled Type |
|------|---------|---------------|
| AnomalyRule | Detection rules with multi-signal scoring | CompiledAnomalyRule |
| EntitySchema | Entity/edge type definitions | CompiledEntitySchema |
| FeatureConfig | 10-dimensional feature vector | CompiledFeatureConfig |
| ScoringConfig | Anomaly scoring weights | CompiledScoringConfig |
| TrendConfig | Trend detection parameters | CompiledTrendConfig |
| PatternConfig | Temporal pattern settings | CompiledPatternConfig |

Each Compiled* type uses HashMap/HashSet for O(1) hot-path lookups.
extends keyword: deep-merge parent YAML, child wins, arrays replace entirely.

## Entity Model (10 Types, 9 Edges)

**Entities**: Member, Device, Game, Affiliate, Currency, VipGroup, Error, Platform, Popup, Provider
**Edges**: LoggedInFrom, OpenedGame, SawPopup, HitError, BelongsToGroup, ReferredBy, UsesCurrency, PlaysOnPlatform, ProvidedBy
**Events**: Login, GameOpened, PopupModule, "API Error"

## Key File Paths

| Component | Path |
|-----------|------|
| Workspace Cargo.toml | Cargo.toml |
| Core types | crates/core/src/ |
| Rule schema | crates/rules/src/schema.rs |
| Rule loader | crates/rules/src/loader.rs |
| Rule validation | crates/rules/src/validation/ |
| Rule YAML files | data/rules/{anomaly,schema,features,scoring,patterns}/ |
| Entity extraction | crates/connector/src/entity_extract.rs |
| Compute pipeline | crates/compute/src/pipeline/ |
| Algorithms | crates/compute/src/algorithms/ |
| Storage engine | crates/storage/src/lib.rs |
| Segment reader/writer | crates/segment/src/ |
| Graph store | crates/graph/src/ |
| LLM providers | crates/llm/src/providers/ |
| AgenticLoop | crates/tool-runtime/src/runtime.rs |
| Tool trait | crates/tool-runtime/src/tool.rs |
| MCP server | crates/mcp/src/server.rs |
| Server main | crates/server/src/main.rs |
| Server API | crates/server/src/api/ |
| Dashboard app | dashboard/app/ |
| Dashboard components | dashboard/components/ |
| Chat bridge | dashboard/app/api/assistant/chat/route.ts |
| Architecture docs | docs/architecture/ |

## Testing

- Use cargo nextest run — parallel per-process isolation, 90% faster
- Integration tests: crates/{name}/tests/
- Unit tests: #[cfg(test)] modules in source files
- Schema sync tests verify YAML entity/edge types match Rust enum variants
- 600+ tests across workspace

## Build Optimization

- sccache for compilation caching (94% hit rate on clean builds)
- incremental = true for dev profile (sccache won't cache these — expected)
- lld linker for faster linking
- cargo-nextest for parallel test execution

## Agent System Architecture

AgenticLoop (tool-runtime):
- ToolRegistry → [BashExecute, FileRead, FileWrite, GraphQuery, RuleList, RuleEvaluate]
- PermissionChecker → PolicyChecker (auto-approve / confirm / deny per tool)
- ToolAwareLlmProvider → LlmProviderBridge → SimpleLlmProvider → LlmProviderAdapter
- Conversation → [User, Assistant(text + tool_calls)]

Stream events: TextDelta | ToolCallStart/Delta/End | ToolExecutionStart/Result | MessageEnd | Error

Server endpoint: POST /sessions/{id}/stream → SSE
Dashboard bridge: POST /api/assistant/chat → translates Rust SSE → AI SDK v6 events
