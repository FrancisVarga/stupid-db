# Next.js Dashboard Rules

## Framework
- Next.js 16 with App Router — no Pages Router
- React 19 with React Compiler
- TypeScript 5 strict mode
- Tailwind CSS 4 for styling

## Design Philosophy
- Chat-first interface — NOT traditional BI panels with dropdowns/filters
- No authentication required — internal/trusted network deployment
- Dashboard communicates with Rust backend via API proxy routes

## Visualization
- Use D3.js (v7) for ALL visualizations — NEVER Chart.js, Recharts, or other libraries
- D3 force-directed graphs for knowledge graph visualization
- D3 sankey diagrams for flow analysis
- D3 heatmaps, treemaps, scatter plots for analytics

## Component Patterns
- Use refreshKey pattern for parent-child component sync
  - Parent increments key state, child useEffect re-fetches on key change
- 4-layer data flow: Form → Next.js proxy → Rust backend (encrypted JSON) → Dashboard
- API routes proxy to Rust backend at API_BASE (default: http://localhost:3088)

## AI SDK Integration
- Use @ai-sdk/react v3 with useChat() hook
- Per-session transport with X-Session-Id header
- Bridge route translates Rust SSE StreamEvents → AI SDK v6 event format
- Stream event mapping:
  - TextDelta → text-start/text-delta/text-end
  - ToolCallStart/Delta/End → tool-input-start/delta/available
  - ToolExecutionResult → tool-output-available/error

## Component Organization
- dashboard/app/ — Page routes (App Router)
- dashboard/app/api/ — API proxy routes to Rust backend
- dashboard/components/ — Reusable components (assistant, chat, db, viz)
- dashboard/lib/ — Shared utilities (api.ts, ws.ts, d3Utils.ts)

## Code Editors
- Use CodeMirror (@uiw/react-codemirror) for SQL, YAML, JSON editing
- Support multi-language syntax highlighting
