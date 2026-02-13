# ADR-004: LLM-Powered Natural Language Query Interface

## Status
**Accepted**

## Context

Users need to query across three different store types (document, vector, graph) plus computed results. Traditional query languages (SQL, Cypher, vector search DSL) would require users to know which store to query and how. The dashboard should be accessible to non-technical users.

## Decision

The primary query interface is **natural language**, powered by an LLM (OpenAI, Claude, or Ollama). The LLM generates structured **query plans** (not raw SQL/Cypher), which the engine executes across all stores.

## Rationale

### Unified Interface
- Users don't need to know about the three stores
- "Find members who logged in on mobile and played slots" spans document + graph
- The LLM figures out which stores to query and how to combine results

### Query Plans over Raw Queries
- LLM generates a structured JSON plan, not SQL
- Plans are validated, sandboxed, and predictable
- No SQL injection, no runaway queries
- Plans can be cached, replayed, and shared

### Catalog Awareness
- LLM has access to the schema registry (what fields exist per event type)
- LLM knows about computed results (cluster names, community IDs)
- LLM can suggest follow-up questions based on available data

### Multiple LLM Backends
- OpenAI for highest quality
- Claude for reasoning-heavy queries
- Ollama for fully local/private deployments
- Pluggable — no vendor lock-in

## Consequences

- LLM latency added to every query (1-3 seconds for plan generation)
- LLM costs for cloud providers
- Plan validation logic needed to prevent nonsensical plans
- Need good system prompts with schema context
- Conversation state management for follow-up queries

## Alternatives Considered

| Approach | Rejected Because |
|----------|-----------------|
| **SQL** | Doesn't span graph/vector stores naturally |
| **GraphQL** | Too structured for exploratory analytics |
| **Custom DSL** | Learning curve for users, still limited |
| **Point-and-click UI** | Not flexible enough for complex cross-store queries |
