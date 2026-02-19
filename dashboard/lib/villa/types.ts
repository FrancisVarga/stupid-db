// Villa layout engine — shared types between frontend and Rust backend.

/** Widget type discriminant. Each variant maps to a React component in the registry. */
export type WidgetType =
  | "stats-card"
  | "time-series"
  | "data-table"
  | "force-graph"
  | "bar-chart"
  | "scatter-plot"
  | "heatmap"
  | "sankey"
  | "treemap"
  | "anomaly-chart"
  | "trend-chart"
  | "page-rank"
  | "degree-chart";

/** How a widget fetches its data — REST polling or WebSocket push. */
export interface DataSourceConfig {
  type: "api" | "websocket";
  endpoint: string;
  params?: Record<string, string>;
  wsMessageType?: string;
  refreshInterval?: number;
}

/** Grid position & size (grid-unit coordinates, not pixels). */
export interface LayoutPosition {
  x: number;
  y: number;
  w: number;
  h: number;
  minW?: number;
  minH?: number;
}

/** A single widget on the dashboard grid. */
export interface WidgetConfig {
  id: string;
  type: WidgetType;
  title: string;
  dataSource: DataSourceConfig;
  layout: LayoutPosition;
  props?: Record<string, unknown>;
}

/** An LLM-proposed mutation to the current layout. */
export interface LayoutAction {
  action: "add" | "remove" | "resize" | "move";
  widgetId?: string;
  widget?: WidgetConfig;
  dimensions?: { w: number; h: number };
}

/** POST body sent to /api/villa/suggest. */
export interface VillaSuggestRequest {
  message: string;
  current_layout: WidgetConfig[];
  conversation_id?: string;
}

/** Response from /api/villa/suggest. */
export interface VillaSuggestResponse {
  actions: LayoutAction[];
  explanation: string;
}

/** A single message in the Villa chat sidebar. */
export interface ChatMessage {
  id: string;
  role: "user" | "assistant";
  content: string;
  actions?: LayoutAction[];
  timestamp: number;
}

/** A named dashboard containing its own widgets and chat history. */
export interface Dashboard {
  id: string;
  name: string;
  widgets: WidgetConfig[];
  chatMessages: ChatMessage[];
  createdAt: number;
}
