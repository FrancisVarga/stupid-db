# Query System Overview

## Overview

The query system translates natural language questions into structured query plans that execute across all three stores (document, vector, graph) plus computed knowledge. Users never write SQL, Cypher, or vector search DSL — they talk to an AI that understands the data.

## Query Flow

```mermaid
flowchart TD
    A[User: Natural Language Question] --> B[LLM: Parse Intent + Generate Query Plan]
    B --> C[Validate Query Plan]
    C --> D[Execute Plan Steps]
    D --> E{Step Type}
    E -->|document_scan| F[Document Store Scan]
    E -->|vector_search| G[Vector Index Search]
    E -->|graph_traverse| H[Graph Traversal]
    E -->|compute_read| I[Read Computed Knowledge]
    E -->|aggregate| J[Aggregate / Transform]
    F --> K[Collect Results]
    G --> K
    H --> K
    I --> K
    J --> K
    K --> L[LLM: Summarize + Choose Visualizations]
    L --> M[Response: Text + Render Specs + Data]
```

## Conversation Context

Queries are stateful — follow-ups build on previous results:

```mermaid
sequenceDiagram
    User->>LLM: "Which members had errors last week?"
    LLM->>Engine: Query Plan (doc scan + aggregate)
    Engine->>LLM: 342 members, breakdown by error type
    LLM->>User: Report with chart
    User->>LLM: "Compare them to the healthy cohort"
    Note over LLM: Knows "them" = 342 members from before
    LLM->>Engine: Query Plan (compare sets)
    Engine->>LLM: Feature comparison
    LLM->>User: Comparison report
```

The conversation context is managed by the query session:

```rust
struct QuerySession {
    id: SessionId,
    messages: Vec<Message>,
    // Named result sets from previous queries
    result_sets: HashMap<String, ResultSet>,
    // Last query's results (for "those", "them", "these")
    last_result: Option<ResultSet>,
    created_at: DateTime<Utc>,
}
```

## Components

- [Query Plan](./query-plan.md) — Structure and execution of query plans
- [Catalog](./catalog.md) — Knowledge catalog: what's available to query
- [LLM Integration](./llm-integration.md) — How the LLM generates and summarizes
