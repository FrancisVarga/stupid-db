---
name: dashboard-patterns
description: Next.js dashboard patterns including chat-first UI, D3.js visualizations, and API proxy architecture
triggers:
  - dashboard
  - frontend
  - Next.js
  - D3.js
  - chat interface
  - visualization
  - API proxy
---

# Dashboard Patterns

## Chat-First Architecture

The dashboard is a chat-first interface, not a traditional BI tool. Users interact primarily through natural language, with visualizations generated dynamically.

## AI SDK v6 Streaming Bridge

The most complex pattern is the SSE streaming bridge:

```
Browser (useChat hook)
  → POST /api/assistant/chat (Next.js)
    → POST /sessions/{id}/stream (Rust backend)
      → AgenticLoop runs, streams SSE events
    ← Translate Rust StreamEvent → AI SDK v6 format
  ← AI SDK updates UI reactively
```

### Stream Event Translation

| Rust StreamEvent | AI SDK Event |
|-----------------|--------------|
| TextDelta | text-start → text-delta → text-end |
| ToolCallStart | tool-input-start |
| ToolCallDelta | tool-input-delta |
| ToolCallEnd | tool-input-available |
| ToolExecutionResult | tool-output-available / tool-output-error |
| MessageEnd | finish |

### Key File
`dashboard/app/api/assistant/chat/route.ts` — The bridge implementation.

## Component Sync Pattern

Parent-child refresh via key increment:

```tsx
const [refreshKey, setRefreshKey] = useState(0);

// Parent triggers refresh
const handleRefresh = () => setRefreshKey(k => k + 1);

// Child re-fetches on key change
useEffect(() => {
  fetchData();
}, [refreshKey]);
```

## 4-Layer Data Flow

```
Form (user input)
  → Next.js API route (proxy + validation)
    → Rust backend (business logic + encrypted storage)
      → Response (JSON or SSE stream)
    ← Transform for frontend consumption
  ← React state update → re-render
```

## D3.js Visualization Patterns

All charts use D3.js v7 directly — never wrapper libraries:

- **Force Graph** — Knowledge graph with entity nodes and relationship edges
- **Sankey Diagram** — Data flow visualization
- **Bar/Line Charts** — Time series anomaly scores
- **Heatmaps** — Co-occurrence matrices
- **Treemaps** — Entity distribution

## Dashboard Pages

| Page | Route | Purpose |
|------|-------|---------|
| Home | `/` | System stats, graph metrics, compute health |
| Assistant | `/assistant` | AI chat with streaming + tool execution |
| Agents | `/agents` | Agent configuration and execution |
| DB Browser | `/db` | Database schema tree, query panel, CRUD |
| Athena | `/athena` | AWS Athena query interface |
| Anomalies | `/anomalies` | Detection results visualization |
| Patterns | `/patterns` | Temporal pattern analysis |
| Explorer | `/explore` | Interactive graph explorer |
| Catalog | `/catalog` | Entity catalog browser |
| Queue | `/queue` | Message queue monitoring |
| Reports | `/reports` | Generated report viewer |
