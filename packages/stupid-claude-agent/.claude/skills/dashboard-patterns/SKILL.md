---
name: Dashboard Patterns
description: Next.js + D3.js dashboard patterns for stupid-db. Chat-first UI, D3 visualization patterns, SSE streaming, WebSocket, dark theme, and component architecture. Use when working on the dashboard frontend.
version: 1.0.0
---

# Dashboard Patterns

## Core Principle: Chat-First

The dashboard is NOT a traditional BI tool. There are no dropdown filters, no sidebar navigation, no predefined dashboards. It's a **chat interface** where users type questions and get visualizations back.

```
User: "Show me game activity trends this week"
→ POST /api/query (SSE stream)
→ LLM generates QueryPlan
→ Executor runs against stores
→ Results returned as render specs
→ Dashboard renders D3 visualizations inline in chat
```

## Technology Stack

| Tech | Version | Usage |
|------|---------|-------|
| Next.js | 16.x | App Router (Server + Client Components) |
| React | 19.x | React Compiler enabled |
| D3.js | 7.x | ALL visualizations |
| Tailwind CSS | 4.x | Styling (dark theme) |
| TypeScript | 5.x | Strict mode |

**Never use Chart.js or any other charting library. D3.js only.**

## Component Architecture

### Chat Flow
```
page.tsx → ChatPanel → [MessageBubble, RenderBlockView, QueryInput]
                                ↓
                    Maps render spec type → viz component
                                ↓
                [BarChart, LineChart, ForceGraph, DataTable, ...]
```

### Render Spec
Backend returns structured render specs in SSE stream:
```typescript
interface RenderBlock {
  type: 'bar_chart' | 'line_chart' | 'scatter_plot' | 'force_graph' |
        'sankey' | 'heatmap' | 'treemap' | 'table' | 'insight_card' |
        'trend_chart' | 'pattern_list' | 'anomaly_chart';
  title: string;
  data: unknown;
  config?: Record<string, unknown>;
}
```

## D3 Visualization Pattern

```typescript
'use client';
import { useRef, useEffect } from 'react';
import * as d3 from 'd3';

interface BarChartProps {
  data: Array<{ label: string; value: number }>;
  width?: number;
  height?: number;
}

export function BarChart({ data, width = 600, height = 400 }: BarChartProps) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll('*').remove(); // Always clean previous render

    const margin = { top: 20, right: 20, bottom: 40, left: 60 };
    const innerW = width - margin.left - margin.right;
    const innerH = height - margin.top - margin.bottom;

    const g = svg.append('g')
      .attr('transform', `translate(${margin.left},${margin.top})`);

    // Scales
    const x = d3.scaleBand()
      .domain(data.map(d => d.label))
      .range([0, innerW])
      .padding(0.2);

    const y = d3.scaleLinear()
      .domain([0, d3.max(data, d => d.value) ?? 0])
      .nice()
      .range([innerH, 0]);

    // Axes
    g.append('g').attr('transform', `translate(0,${innerH})`).call(d3.axisBottom(x));
    g.append('g').call(d3.axisLeft(y));

    // Bars with transition
    g.selectAll('rect')
      .data(data)
      .join('rect')
      .attr('x', d => x(d.label) ?? 0)
      .attr('width', x.bandwidth())
      .attr('y', innerH)
      .attr('height', 0)
      .attr('fill', '#6366f1') // Indigo for dark theme
      .transition()
      .duration(500)
      .attr('y', d => y(d.value))
      .attr('height', d => innerH - y(d.value));

  }, [data, width, height]);

  return <svg ref={svgRef} width={width} height={height} className="w-full" />;
}
```

### Key Rules
- `'use client'` directive — D3 requires DOM access
- `useRef` for SVG container
- `selectAll('*').remove()` — clean previous render on every update
- `useEffect` with `[data, width, height]` dependencies
- Handle empty data: `if (!data.length) return`
- Transitions for smooth updates
- Dark theme colors (indigo, emerald, amber on dark backgrounds)

## SSE Streaming Pattern

```typescript
async function streamQuery(query: string, sessionId: string) {
  const response = await fetch('http://localhost:3001/api/query', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ query, session_id: sessionId }),
  });

  const reader = response.body?.getReader();
  const decoder = new TextDecoder();

  while (reader) {
    const { done, value } = await reader.read();
    if (done) break;

    const text = decoder.decode(value);
    // Parse SSE format: "data: {...}\n\n"
    const lines = text.split('\n');
    for (const line of lines) {
      if (line.startsWith('data: ')) {
        const data = JSON.parse(line.slice(6));
        // data contains render blocks, text, follow-up suggestions
        handleRenderBlock(data);
      }
    }
  }
}
```

## WebSocket Pattern (Insights Feed)

```typescript
'use client';
import { useEffect, useRef, useCallback } from 'react';

export function useInsightsFeed(onInsight: (insight: Insight) => void) {
  const wsRef = useRef<WebSocket | null>(null);

  useEffect(() => {
    const ws = new WebSocket('ws://localhost:3001/ws/insights');

    ws.onmessage = (event) => {
      const insight = JSON.parse(event.data);
      onInsight(insight);
    };

    ws.onclose = () => {
      // Reconnect after delay
      setTimeout(() => { /* reconnect logic */ }, 3000);
    };

    wsRef.current = ws;
    return () => ws.close();
  }, [onInsight]);
}
```

## Dark Theme Colors

```
Background:  bg-gray-950, bg-gray-900
Surface:     bg-gray-800, bg-gray-800/50
Text:        text-gray-100, text-gray-400
Accent:      text-indigo-400, text-emerald-400, text-amber-400
Border:      border-gray-700, border-gray-800
```

D3 chart colors for dark backgrounds:
```
Primary:   #6366f1 (indigo)
Secondary: #10b981 (emerald)
Tertiary:  #f59e0b (amber)
Danger:    #ef4444 (red)
Info:      #3b82f6 (blue)
```

## Conventions

- Server Components by default, `'use client'` only for D3/interactivity
- `@/*` path alias for imports
- No authentication (trusted internal network)
- TypeScript strict mode — no `any`
- Tailwind utility classes only, no custom CSS
- All visualizations handle empty data gracefully
- Responsive: use container queries or viewBox for SVGs
