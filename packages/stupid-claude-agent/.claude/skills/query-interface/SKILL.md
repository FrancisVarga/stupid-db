---
name: Query Interface
description: Query planning and execution patterns for stupid-db. QueryPlan model, multi-store execution, LLM integration for plan generation, catalog summaries, and structured output. Use when working on the query system or LLM integration.
version: 1.0.0
---

# Query Interface

## Query Flow

```
User Question
    ↓
LLM (with catalog context) → generates QueryPlan (JSON)
    ↓
PlanValidator → checks references, fields, depth limits
    ↓
PlanExecutor → runs steps against Document/Vector/Graph stores
    ↓
Result Merge → combines multi-store results
    ↓
LLM (synthesis) → natural language summary + render specs
    ↓
SSE Stream → dashboard receives render blocks incrementally
```

**Key insight**: The LLM generates structured QueryPlans, NOT SQL. The executor knows how to run plans against all three stores.

## QueryPlan Model

```rust
pub struct QueryPlan {
    pub steps: Vec<QueryStep>,
    pub merge_strategy: MergeStrategy,
}

pub enum QueryStep {
    DocumentScan { filter: Filter, projection: Vec<String>, limit: Option<usize> },
    VectorSearch { query_text: String, top_k: usize, filter: Option<Filter> },
    GraphTraversal { start_node: NodeSelector, traversal: TraversalSpec, max_depth: usize },
    ComputeRead { result_type: ComputeResultType, filter: Option<Filter> },
    Aggregate { input_step: usize, aggregations: Vec<Aggregation>, group_by: Vec<String> },
}

pub enum MergeStrategy {
    Sequential,   // Steps run in order, each can reference prior results
    Parallel,     // Independent steps run concurrently
    Enrich,       // Primary result enriched with secondary results
}
```

## Plan Examples

### "Which members played the most games yesterday?"
```json
{
  "steps": [
    {
      "type": "DocumentScan",
      "filter": { "eventType": "GameOpened", "timestamp_gte": "yesterday" },
      "projection": ["memberCode", "gameUid"]
    },
    {
      "type": "Aggregate",
      "input_step": 0,
      "group_by": ["memberCode"],
      "aggregations": [{ "type": "count", "field": "gameUid", "alias": "game_count" }]
    }
  ],
  "merge_strategy": "sequential"
}
```

### "Find members similar to M12345"
```json
{
  "steps": [
    { "type": "VectorSearch", "query_text": "member:M12345", "top_k": 20 }
  ],
  "merge_strategy": "single"
}
```

### "What communities exist and who are the key members?"
```json
{
  "steps": [
    { "type": "ComputeRead", "result_type": "communities" },
    {
      "type": "GraphTraversal",
      "start_node": { "type": "community_centers" },
      "traversal": { "type": "neighborhood", "edge_types": ["PLAYS", "USES_DEVICE"] },
      "max_depth": 2
    }
  ],
  "merge_strategy": "enrich"
}
```

## LLM System Prompt Structure

```
You are a query planner for stupid-db, a knowledge materialization engine
with document, vector, and graph stores.

## Available Data
{catalog_summary}

## QueryPlan Schema
{json_schema}

## Step Types
- DocumentScan: Filter and project from raw events
- VectorSearch: Semantic similarity search
- GraphTraversal: Walk entity relationships
- ComputeRead: Read pre-computed results (clusters, communities, anomalies)
- Aggregate: Group and aggregate previous step results

## Examples
{few_shot_examples}

## Constraints
- GraphTraversal max_depth must be <= 5
- VectorSearch top_k must be <= 100
- Always validate field names against the catalog
- Prefer ComputeRead for questions about patterns/clusters/anomalies
```

## Catalog Summary

The catalog provides context to the LLM:

```rust
pub struct CatalogSummary {
    pub event_types: Vec<EventTypeSummary>,     // name, fields, count
    pub entity_types: Vec<EntityTypeSummary>,    // type, count, sample values
    pub compute_results: ComputeResultSummary,   // available clusters, communities, etc.
    pub data_range: TimeRange,                   // earliest/latest timestamps
    pub total_documents: u64,
}
```

**Keep under token limits**: The summary is injected into every LLM call. It must be concise but complete enough for the LLM to generate valid plans.

## Response Synthesis

After plan execution, the LLM synthesizes results into:

1. **Text summary**: "Here are the top 10 most active members this week..."
2. **Render specs**: JSON objects mapping to dashboard visualizations
3. **Follow-up suggestions**: "You might also want to explore..."

```json
{
  "text": "The most active members this week are...",
  "render_blocks": [
    {
      "type": "bar_chart",
      "title": "Top Members by Game Count",
      "data": [{"label": "M12345", "value": 142}, ...]
    }
  ],
  "follow_ups": [
    "What games do these members play?",
    "Are any of these members in the same community?"
  ]
}
```

## Provider Support

| Provider | Plan Generation | Response Synthesis | Labeling |
|----------|----------------|-------------------|----------|
| OpenAI | JSON mode / function calling | Chat completion | Function calling |
| Anthropic | Tool use / function calling | Messages API | Tool use |
| Ollama | JSON output with validation | Chat completion | JSON parsing |

**Never hardcode a single provider.** All three must work via the `LlmBackend` trait.

## Validation Rules

- Step references must be backward (no forward references)
- Field names must exist in catalog schema
- Graph traversals bounded by max_depth
- Vector search bounded by top_k
- Aggregate input_step must reference a valid prior step
- Filter values must match field types
