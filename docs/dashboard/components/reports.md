# Reports

## Overview

A report is a **saved conversation** — a sequence of questions and AI responses with their inline visualizations. Reports can be bookmarked, shared via URL, and exported.

## Report Model

```typescript
interface Report {
  id: string;                    // UUID
  title: string;                 // Auto-generated from first question, editable
  session_id: string;            // Query session this report belongs to
  messages: Message[];           // Full conversation
  created_at: string;
  updated_at: string;
  // Snapshot of data at report creation time
  data_range: {
    start: string;
    end: string;
    segments: string[];
  };
}
```

## Saving a Report

Any conversation can be saved as a report:

```mermaid
flowchart LR
    A[Chat Conversation] -->|User clicks Save| B[Generate Report]
    B --> C[Store in Rust Backend]
    C --> D[Return Permalink URL]
    D --> E[/reports/abc123]
```

## Report Page (`/reports/[id]`)

Renders the saved conversation read-only, with all visualizations intact:

```
┌─────────────────────────────────────────────┐
│  Report: Members who churned after errors    │
│  Created: 2025-07-12 14:32                   │
│  Data range: Jun 12 - Jul 12, 2025           │
│  [Export PDF] [Export CSV] [Continue Chat]    │
├─────────────────────────────────────────────┤
│                                              │
│  Q: Which members had errors and stopped...  │
│                                              │
│  A: Found 342 members...                     │
│     [Bar Chart]                              │
│     [Graph Visualization]                    │
│     Top segments: VIPB (41%)...              │
│                                              │
│  Q: Compare to healthy cohort                │
│                                              │
│  A: Comparison shows...                      │
│     [Comparison Chart]                       │
│     [Data Table]                             │
│                                              │
└─────────────────────────────────────────────┘
```

## Export Formats

### CSV Export
Any data table or result set in the conversation:
```typescript
function exportCSV(data: any[], filename: string) {
  const csv = convertToCSV(data);
  downloadBlob(csv, `${filename}.csv`, 'text/csv');
}
```

### PNG/SVG Export
Any D3 chart:
```typescript
function exportChart(svgElement: SVGElement, filename: string, format: 'png' | 'svg') {
  if (format === 'svg') {
    const svgData = new XMLSerializer().serializeToString(svgElement);
    downloadBlob(svgData, `${filename}.svg`, 'image/svg+xml');
  } else {
    // Convert SVG to canvas → PNG
    const canvas = svgToCanvas(svgElement);
    canvas.toBlob(blob => downloadBlob(blob, `${filename}.png`, 'image/png'));
  }
}
```

### PDF Export
Full report with all visualizations:
- Client-side: Use html2canvas + jsPDF
- Renders each message block sequentially
- Charts converted to images

## Storage

Reports are stored in the Rust backend:

```
data/reports/
├── abc123.json     # Report metadata + messages
├── def456.json
└── ...
```

### API

```
POST   /api/reports              → Create report from session
GET    /api/reports              → List all reports
GET    /api/reports/:id          → Get report by ID
DELETE /api/reports/:id          → Delete report
GET    /api/reports/:id/export   → Export as PDF/CSV
```

## Continue Chat

From a saved report, the user can click "Continue Chat" to resume the conversation in the main chat view, with full context preserved.
