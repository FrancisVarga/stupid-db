---
name: LLM Prompt Patterns
description: LLM prompt engineering patterns for stupid-db. System prompts for query planning, response synthesis, entity labeling, and provider-agnostic structured output. Use when designing or modifying LLM prompts.
version: 1.0.0
---

# LLM Prompt Patterns

## Provider-Agnostic Design

All prompts must work across three providers:

| Provider | Structured Output | Best Practice |
|----------|-------------------|---------------|
| OpenAI (GPT-4o) | JSON mode, function calling | Use `response_format: { type: "json_object" }` |
| Anthropic (Claude) | Tool use, function calling | Use tool definitions with JSON schema |
| Ollama (local) | Raw JSON parsing | Include JSON schema in prompt, validate output |

**Architecture**: `LlmBackend` trait wraps provider differences. Prompts are provider-agnostic; the backend handles format conversion.

## Query Planning Prompt

### System Prompt Template
```
You are a query planner for stupid-db, a knowledge materialization engine.
Your job is to convert natural language questions into structured QueryPlans.

## Available Data (from catalog)
Event Types:
{{#each event_types}}
- {{name}}: {{doc_count}} documents, fields: [{{fields}}]
{{/each}}

Entity Types:
{{#each entity_types}}
- {{type}}: {{count}} entities (e.g., {{samples}})
{{/each}}

Pre-Computed Results:
{{#if clusters}}- {{cluster_count}} clusters available{{/if}}
{{#if communities}}- {{community_count}} communities detected{{/if}}
{{#if anomalies}}- {{anomaly_count}} active anomalies{{/if}}
{{#if patterns}}- {{pattern_count}} sequential patterns found{{/if}}

Data Range: {{start_date}} to {{end_date}}
Total Documents: {{total_docs}}

## QueryPlan Format
Return a JSON object with this schema:
{
  "steps": [QueryStep, ...],
  "merge_strategy": "sequential" | "parallel" | "enrich"
}

QueryStep types:
- DocumentScan: { "type": "DocumentScan", "filter": {...}, "projection": [...], "limit": N }
- VectorSearch: { "type": "VectorSearch", "query_text": "...", "top_k": N }
- GraphTraversal: { "type": "GraphTraversal", "start_node": {...}, "traversal": {...}, "max_depth": N }
- ComputeRead: { "type": "ComputeRead", "result_type": "clusters|communities|anomalies|patterns|trends" }
- Aggregate: { "type": "Aggregate", "input_step": N, "group_by": [...], "aggregations": [...] }

## Rules
1. Use ComputeRead for pre-computed insights (clusters, communities, anomalies)
2. Use DocumentScan for raw event queries
3. Use VectorSearch for similarity-based questions
4. Use GraphTraversal for relationship questions
5. max_depth <= 5 for graph traversals
6. top_k <= 100 for vector search
7. Always reference valid field names from the catalog
```

### Few-Shot Examples
Include 3-5 diverse examples covering each step type:

```
Q: "Show me the most active members this week"
A: {"steps":[{"type":"DocumentScan","filter":{"eventType":"GameOpened","@timestamp":{"gte":"now-7d"}},"projection":["memberCode"]},{"type":"Aggregate","input_step":0,"group_by":["memberCode"],"aggregations":[{"type":"count","alias":"games"}]}],"merge_strategy":"sequential"}

Q: "What communities have been detected?"
A: {"steps":[{"type":"ComputeRead","result_type":"communities"}],"merge_strategy":"single"}

Q: "How is member M12345 connected to member M67890?"
A: {"steps":[{"type":"GraphTraversal","start_node":{"type":"Member","id":"M12345"},"traversal":{"type":"shortest_path","target":{"type":"Member","id":"M67890"}},"max_depth":4}],"merge_strategy":"single"}
```

## Response Synthesis Prompt

```
You are a data analyst for a gaming platform. Given query results, create:
1. A clear natural language summary (2-3 sentences)
2. Visualization specifications (render blocks)
3. Follow-up question suggestions (2-3)

## Render Block Types
- bar_chart: { type, title, data: [{label, value}] }
- line_chart: { type, title, data: [{x, y, series?}] }
- scatter_plot: { type, title, data: [{x, y, label?}] }
- force_graph: { type, title, data: {nodes: [...], edges: [...]} }
- table: { type, title, data: {columns: [...], rows: [...]} }
- sankey: { type, title, data: {nodes: [...], links: [...]} }
- heatmap: { type, title, data: {rows: [...], cols: [...], values: [...]} }
- insight_card: { type, title, data: {metric, value, change, trend} }

Choose the visualization that best represents the data.

## Results
{{query_results}}

## Original Question
{{user_question}}
```

## Labeling Prompt

For labeling clusters, communities, and anomalies:

```
You are labeling groups of gaming platform entities.
Given the characteristics below, provide a concise label (3-5 words).

Group characteristics:
- Entity types: {{entity_types}}
- Top members: {{top_members}}
- Common games: {{common_games}}
- Activity level: {{activity_level}}
- Platform distribution: {{platforms}}

Provide ONLY the label, nothing else.
Example labels:
- "High-activity mobile gamers"
- "Slot enthusiasts group"
- "Inactive desktop users"
- "Multi-provider power players"
```

## Prompt Engineering Tips

### Token Efficiency
- Catalog summary should be < 500 tokens
- Few-shot examples: 3-5 is optimal (diminishing returns after)
- Trim unnecessary fields from examples

### Reliability
- Always include JSON schema for structured output
- Add negative examples: "Do NOT generate SQL queries"
- Validate LLM output against schema before execution
- Retry with feedback on validation failure

### Provider-Specific Notes
- **OpenAI**: Use `json_object` response format for reliability
- **Anthropic**: Tool use definitions enforce schema; prefer this over raw JSON
- **Ollama**: Include "Respond ONLY with valid JSON" instruction; parse and validate

### Session Context
For follow-up queries, include prior context:
```
Previous question: "Show me active members"
Previous results: [summary of prior results]
Current question: "What games do they play?"
```

The "they" resolves to members from the prior result set.
