# D3.js Visualization Components

## Overview

Each visualization component is a React wrapper around D3.js. They receive data and configuration from the backend's render specs and produce interactive, inline charts within the chat interface.

## Component Catalog

### BarChart

**Use when**: Distributions, comparisons, timelines with discrete buckets.

```typescript
interface BarChartProps {
  data: Array<{ label: string; value: number; color?: string }>;
  title: string;
  x: string;      // Field name for x-axis
  y: string;      // Field name for y-axis
  orientation?: 'vertical' | 'horizontal';
  stacked?: boolean;
  groupBy?: string;
}
```

**Features**:
- Hover tooltip with exact values
- Click bar → emit event for drill-down
- Animated transitions when data updates
- Auto-scale axes

### LineChart

**Use when**: Trends over continuous time.

```typescript
interface LineChartProps {
  data: Array<{ timestamp: string; value: number; series?: string }>;
  title: string;
  x: string;
  y: string;
  multiSeries?: boolean;
  showArea?: boolean;
  annotations?: Array<{ x: string; label: string }>;  // Vertical markers
}
```

**Features**:
- Multi-series with legend
- Brush selection for time range zoom
- Annotation markers (e.g., "deployment here")
- Responsive width

### ScatterPlot

**Use when**: Cluster visualization, 2D feature projection.

```typescript
interface ScatterPlotProps {
  data: Array<{ x: number; y: number; label?: string; cluster?: number; size?: number }>;
  title: string;
  colorBy?: string;   // Field to color points by
  sizeBy?: string;    // Field to size points by
  showCentroids?: boolean;
}
```

**Features**:
- Color-coded by cluster/category
- Zoom and pan
- Hover shows point details
- Click selects point → drill-down
- Optional centroid markers
- Lasso selection for multi-point queries

### ForceGraph

**Use when**: Entity relationships, neighborhood exploration.

```typescript
interface ForceGraphProps {
  nodes: Array<{
    id: string;
    label: string;
    type: string;       // Entity type → determines color/shape
    size?: number;       // Node size (e.g., PageRank score)
  }>;
  edges: Array<{
    source: string;
    target: string;
    type: string;        // Edge type → determines style
    weight?: number;     // Edge thickness
  }>;
  title: string;
  highlightNode?: string;
}
```

**Features**:
- Force-directed layout (D3 force simulation)
- Color-coded by entity type:
  - Member: blue circles
  - Game: green diamonds
  - Device: gray squares
  - VipGroup: gold hexagons
  - Error: red triangles
- Edge types shown as different line styles (solid, dashed, dotted)
- Edge weight → line thickness
- Drag nodes to rearrange
- Zoom and pan
- Click node → show details + neighbors
- Double-click → expand neighborhood (load more connections)

### SankeyDiagram

**Use when**: Funnels, flow analysis (login → game → action).

```typescript
interface SankeyProps {
  nodes: Array<{ id: string; label: string }>;
  links: Array<{ source: string; target: string; value: number }>;
  title: string;
}
```

**Features**:
- Flow width proportional to count
- Hover shows exact flow values
- Click a link → drill into that segment

### Heatmap

**Use when**: Correlation matrices, time × category analysis.

```typescript
interface HeatmapProps {
  data: Array<{ x: string; y: string; value: number }>;
  title: string;
  colorScale?: 'sequential' | 'diverging';
  xLabel?: string;
  yLabel?: string;
}
```

**Features**:
- Color intensity maps to value
- Hover shows cell value
- Row/column sorting (by value or alphabetical)

### Treemap

**Use when**: Hierarchical breakdowns (VIP group → currency → game).

```typescript
interface TreemapProps {
  data: {
    name: string;
    children: Array<{
      name: string;
      value?: number;
      children?: TreemapProps['data']['children'];
    }>;
  };
  title: string;
  colorBy?: string;
}
```

**Features**:
- Nested rectangles proportional to value
- Click to zoom into a branch
- Breadcrumb navigation for drill-down

### DataTable

**Use when**: Raw data display, exportable result sets.

```typescript
interface DataTableProps {
  data: Array<Record<string, any>>;
  columns: string[];
  title: string;
  sortable?: boolean;
  filterable?: boolean;
  limit?: number;
  exportable?: boolean;
}
```

**Features**:
- Sortable columns (click header)
- Text filter per column
- Pagination
- Export as CSV button
- Click row → drill-down

### InsightCard

**Use when**: Text summaries, key metrics, anomaly descriptions.

```typescript
interface InsightCardProps {
  title: string;
  text: string;
  metrics?: Array<{ label: string; value: string | number; change?: string }>;
  severity?: 'info' | 'warning' | 'critical';
}
```

**Features**:
- Clean card layout
- Key metrics with change indicators (up/down arrows)
- Severity-based border color
- Collapsible details

## Shared D3 Utilities

```typescript
// Shared across all chart components
const chartUtils = {
  // Responsive SVG container
  createResponsiveSvg(container: HTMLElement, aspectRatio: number): d3.Selection,

  // Common color scales
  categoryColors: d3.scaleOrdinal(d3.schemeTableau10),
  sequentialColors: d3.scaleSequential(d3.interpolateViridis),
  divergingColors: d3.scaleSequential(d3.interpolateRdYlBu),

  // Entity type → color mapping
  entityColors: {
    Member: '#4A90D9',
    Game: '#27AE60',
    Device: '#95A5A6',
    VipGroup: '#F39C12',
    Error: '#E74C3C',
    Affiliate: '#9B59B6',
    Currency: '#1ABC9C',
    Platform: '#3498DB',
  },

  // Tooltip helper
  createTooltip(): d3.Selection,

  // Number formatting
  formatNumber(n: number): string,  // 1234567 → "1.2M"
  formatPercent(n: number): string, // 0.42 → "42%"
};
```

## Responsive Sizing

Charts auto-resize based on the chat message width:

```typescript
const CHART_CONFIG = {
  maxWidth: 700,      // Max chart width in pixels
  aspectRatio: 16/9,  // Default aspect ratio
  minHeight: 200,     // Minimum chart height
  maxHeight: 500,     // Maximum chart height
  margin: { top: 30, right: 20, bottom: 40, left: 50 },
};
```

## Animation

- Charts animate on mount (bars grow, lines draw, nodes settle)
- Transitions on data update (smooth resizing, color changes)
- Force graph has continuous physics simulation
- All animations respect `prefers-reduced-motion`
