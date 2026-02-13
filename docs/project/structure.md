# Project Structure

## Repository Layout

```
stupid-db/
├── Cargo.toml                    # Workspace root
├── Cargo.lock
├── config/
│   ├── default.toml              # Default configuration
│   └── development.toml          # Dev overrides
├── crates/                       # Rust workspace members
│   ├── core/                     # Shared types, traits, config
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── document.rs       # Document model
│   │       ├── entity.rs         # Entity types
│   │       ├── config.rs         # Configuration structs
│   │       ├── error.rs          # Error types
│   │       └── traits.rs         # Shared traits
│   ├── segment/                  # Time-partitioned storage
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── writer.rs         # Active segment writer
│   │       ├── reader.rs         # Segment reader (mmap)
│   │       ├── index.rs          # Document offset index
│   │       ├── rotation.rs       # Segment rotation logic
│   │       └── eviction.rs       # TTL-based segment cleanup
│   ├── vector/                   # Vector embeddings + index
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── hnsw.rs           # HNSW index wrapper
│   │       ├── search.rs         # Cross-segment search + merge
│   │       └── quantization.rs   # Optional scalar/PQ quantization
│   ├── graph/                    # In-memory property graph
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── store.rs          # GraphStore implementation
│   │       ├── traversal.rs      # BFS/DFS/Dijkstra
│   │       ├── pagerank.rs       # PageRank algorithm
│   │       ├── louvain.rs        # Louvain community detection
│   │       └── stats.rs          # Graph statistics
│   ├── ingest/                   # Data ingestion pipeline
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── parquet.rs        # Parquet file reader
│   │       ├── stream.rs         # Streaming event ingestion
│   │       ├── normalize.rs      # Field normalization
│   │       ├── watcher.rs        # File system watcher
│   │       └── remote.rs         # S3/HTTP parquet reader
│   ├── connector/                # Hot-path processing
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── entity_extract.rs # Entity extraction rules
│   │       ├── edge_derive.rs    # Edge derivation rules
│   │       └── feature_vector.rs # Feature vector construction
│   ├── embedder/                 # Embedding generation
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs         # Embedder trait
│   │       ├── onnx.rs           # ONNX Runtime backend
│   │       ├── ollama.rs         # Ollama API backend
│   │       ├── openai.rs         # OpenAI API backend
│   │       ├── batcher.rs        # Embedding batch processor
│   │       └── cache.rs          # Embedding cache
│   ├── compute/                  # Continuous compute engine
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── scheduler.rs      # Priority-based task scheduler
│   │       ├── state.rs          # KnowledgeState
│   │       ├── traits.rs         # ComputeTask trait
│   │       ├── algorithms/
│   │       │   ├── mod.rs
│   │       │   ├── kmeans.rs     # K-means + streaming mini-batch
│   │       │   ├── dbscan.rs     # DBSCAN clustering
│   │       │   ├── temporal.rs   # Temporal pattern mining (PrefixSpan)
│   │       │   ├── cooccurrence.rs # Co-occurrence matrices
│   │       │   ├── anomaly.rs    # Multi-signal anomaly detection
│   │       │   └── trend.rs      # Trend detection (z-score)
│   │       └── insight.rs        # Proactive insight generation
│   ├── catalog/                  # Knowledge catalog
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── schema.rs         # Schema registry
│   │       ├── entity_catalog.rs # Entity catalog
│   │       ├── compute_catalog.rs # Computed knowledge catalog
│   │       └── summary.rs        # Condensed catalog for LLM
│   ├── query/                    # Query planning + execution
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── plan.rs           # QueryPlan model
│   │       ├── executor.rs       # Plan execution engine
│   │       ├── validator.rs      # Plan validation
│   │       ├── session.rs        # Conversation session management
│   │       └── cache.rs          # Plan cache
│   ├── llm/                      # LLM integration
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs         # LlmBackend trait
│   │       ├── openai.rs         # OpenAI backend
│   │       ├── anthropic.rs      # Anthropic backend
│   │       ├── ollama.rs         # Ollama backend
│   │       ├── prompts.rs        # System prompts + templates
│   │       └── labeler.rs        # Cluster/community label generation
│   └── server/                   # HTTP/WS API server
│       ├── Cargo.toml
│       └── src/
│           ├── main.rs           # Entry point
│           ├── api/
│           │   ├── mod.rs
│           │   ├── ingest.rs     # Ingest endpoints
│           │   ├── query.rs      # Query endpoint (SSE)
│           │   ├── catalog.rs    # Catalog browse endpoints
│           │   ├── reports.rs    # Report CRUD
│           │   └── system.rs     # Health, metrics, status
│           ├── ws/
│           │   ├── mod.rs
│           │   └── insights.rs   # WebSocket insight feed
│           └── state.rs          # Shared application state
├── dashboard/                    # Next.js frontend
│   ├── package.json
│   ├── next.config.js
│   ├── tailwind.config.js
│   ├── tsconfig.json
│   ├── app/
│   │   ├── layout.tsx            # Root layout (dark theme shell)
│   │   ├── page.tsx              # Main chat interface
│   │   ├── reports/
│   │   │   └── [id]/
│   │   │       └── page.tsx      # Saved report view
│   │   └── globals.css           # Global styles
│   ├── components/
│   │   ├── chat/
│   │   │   ├── ChatWindow.tsx
│   │   │   ├── MessageBubble.tsx
│   │   │   ├── QueryInput.tsx
│   │   │   └── StreamHandler.tsx
│   │   ├── viz/
│   │   │   ├── BarChart.tsx
│   │   │   ├── LineChart.tsx
│   │   │   ├── ScatterPlot.tsx
│   │   │   ├── ForceGraph.tsx
│   │   │   ├── SankeyDiagram.tsx
│   │   │   ├── Heatmap.tsx
│   │   │   ├── Treemap.tsx
│   │   │   ├── DataTable.tsx
│   │   │   ├── InsightCard.tsx
│   │   │   └── RenderBlock.tsx   # Maps render spec → component
│   │   ├── insights/
│   │   │   ├── InsightFeed.tsx
│   │   │   ├── InsightCard.tsx
│   │   │   └── SystemStatus.tsx
│   │   └── common/
│   │       ├── ExportButton.tsx
│   │       └── Spinner.tsx
│   └── lib/
│       ├── api.ts                # Rust backend API client
│       ├── ws.ts                 # WebSocket connection manager
│       ├── renderSpec.ts         # Render spec → component mapping
│       ├── session.ts            # Chat session state
│       └── d3Utils.ts            # Shared D3 helpers
├── models/                       # Embedding model files
│   └── .gitkeep                  # Models downloaded at runtime
├── data/                         # Data directory
│   ├── segments/                 # Time-partitioned segments
│   ├── reports/                  # Saved reports
│   └── archive/                  # Archived computed results
├── docs/                         # This documentation
└── scripts/
    ├── setup.sh                  # First-time setup
    ├── import-sample.sh          # Import sample data
    └── dev.sh                    # Start dev environment
```

## Workspace Dependencies

```mermaid
graph TD
    server --> query
    server --> ingest
    server --> catalog
    server --> compute
    query --> llm
    query --> catalog
    query --> segment
    query --> vector
    query --> graph
    query --> compute
    ingest --> segment
    ingest --> connector
    connector --> graph
    connector --> embedder
    connector --> vector
    connector --> compute
    catalog --> segment
    catalog --> graph
    catalog --> compute
    compute --> vector
    compute --> graph
    compute --> segment
    segment --> core
    vector --> core
    graph --> core
    ingest --> core
    connector --> core
    embedder --> core
    compute --> core
    catalog --> core
    query --> core
    llm --> core
```
