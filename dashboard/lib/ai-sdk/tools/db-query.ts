import { tool } from "ai";
import { z } from "zod";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

type QueryType =
  | "stats"
  | "graph_nodes"
  | "graph_edges"
  | "anomalies"
  | "patterns"
  | "pagerank"
  | "entity_search"
  | "communities"
  | "degrees"
  | "trends"
  | "cooccurrence";

const MAX_RESULTS = 50;

function buildUrl(path: string, params?: Record<string, string>): string {
  const url = `${API_BASE}${path}`;
  if (!params || Object.keys(params).length === 0) return url;
  const qs = new URLSearchParams(params).toString();
  return `${url}?${qs}`;
}

function truncateResults<T>(items: T[], limit: number): { items: T[]; truncated: number } {
  if (items.length <= limit) return { items, truncated: 0 };
  return { items: items.slice(0, limit), truncated: items.length - limit };
}

async function queryBackend(queryType: QueryType, query?: string, limit = 20) {
  const cap = Math.min(limit, MAX_RESULTS);

  const endpointMap: Record<QueryType, () => string> = {
    stats: () => buildUrl("/stats"),
    graph_nodes: () => buildUrl("/graph/force", { limit: String(cap) }),
    graph_edges: () => buildUrl("/graph/force", { limit: String(cap) }),
    anomalies: () => buildUrl("/compute/anomalies", { limit: String(cap) }),
    patterns: () => buildUrl("/compute/patterns"),
    pagerank: () => buildUrl("/compute/pagerank", { limit: String(cap) }),
    entity_search: () =>
      buildUrl("/graph/force", { limit: String(cap) }),
    communities: () => buildUrl("/compute/communities"),
    degrees: () => buildUrl("/compute/degrees", { limit: String(cap) }),
    trends: () => buildUrl("/compute/trends"),
    cooccurrence: () => {
      const params: Record<string, string> = {};
      if (query) params.entity_type_a = query;
      return buildUrl("/compute/cooccurrence", Object.keys(params).length > 0 ? params : undefined);
    },
  };

  const url = endpointMap[queryType]();
  const res = await fetch(url, { cache: "no-store" });

  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(`Backend returned ${res.status}: ${text || res.statusText}`);
  }

  return res.json();
}

interface ToolResult {
  query_type: string;
  data?: unknown;
  count?: number;
  search_query?: string;
  note?: string;
  error?: string;
  hint?: string;
}

function formatResult(queryType: QueryType, raw: unknown, query?: string, limit = 20): ToolResult {
  const cap = Math.min(limit, MAX_RESULTS);

  switch (queryType) {
    case "stats":
      return { query_type: "stats", data: raw };

    case "graph_nodes": {
      const graph = raw as { nodes: unknown[] };
      const { items, truncated } = truncateResults(graph.nodes, cap);
      return {
        query_type: "graph_nodes",
        count: graph.nodes.length,
        data: items,
        ...(truncated > 0 && { note: `... truncated ${truncated} more nodes` }),
      };
    }

    case "graph_edges": {
      const graph = raw as { links: unknown[] };
      const { items, truncated } = truncateResults(graph.links, cap);
      return {
        query_type: "graph_edges",
        count: graph.links.length,
        data: items,
        ...(truncated > 0 && { note: `... truncated ${truncated} more edges` }),
      };
    }

    case "entity_search": {
      const graph = raw as { nodes: Array<{ entity_type: string; key: string }> };
      const q = (query || "").toLowerCase();
      const filtered = q
        ? graph.nodes.filter(
            (n) => n.key.toLowerCase().includes(q) || n.entity_type.toLowerCase().includes(q),
          )
        : graph.nodes;
      const { items, truncated } = truncateResults(filtered, cap);
      return {
        query_type: "entity_search",
        search_query: query,
        count: filtered.length,
        data: items,
        ...(truncated > 0 && { note: `... truncated ${truncated} more results` }),
      };
    }

    case "anomalies":
    case "patterns":
    case "pagerank":
    case "communities":
    case "degrees":
    case "trends": {
      const arr = Array.isArray(raw) ? raw : [];
      const { items, truncated } = truncateResults(arr, cap);
      return {
        query_type: queryType,
        count: arr.length,
        data: items,
        ...(truncated > 0 && { note: `... truncated ${truncated} more results` }),
      };
    }

    case "cooccurrence": {
      const data = Array.isArray(raw) ? raw : [raw];
      return {
        query_type: "cooccurrence",
        count: data.length,
        data: data.slice(0, cap),
      };
    }

    default:
      return { query_type: queryType, data: raw };
  }
}

const queryTypeSchema = z.enum([
  "stats",
  "graph_nodes",
  "graph_edges",
  "anomalies",
  "patterns",
  "pagerank",
  "entity_search",
  "communities",
  "degrees",
  "trends",
  "cooccurrence",
]);

const parametersSchema = z.object({
  query_type: queryTypeSchema.describe(
    "Type of query: stats (overview), graph_nodes/graph_edges (graph data), " +
    "anomalies (anomaly scores), patterns (temporal patterns), pagerank (importance ranking), " +
    "entity_search (search by name/type), communities (clusters), degrees (connectivity), " +
    "trends (metric trends), cooccurrence (entity co-occurrence)",
  ),
  query: z.string().optional().describe("Search terms for entity_search, or entity_type for cooccurrence filter"),
  limit: z.number().optional().default(20).describe("Max results to return (capped at 50)"),
});

export const dbQueryTool = tool({
  description:
    "Query the stupid-db backend for entities, anomalies, patterns, and graph data. " +
    "Use this when the user asks about data, entities, patterns, anomalies, or wants to explore the database.",
  inputSchema: parametersSchema,
  execute: async ({ query_type, query, limit }): Promise<ToolResult> => {
    try {
      const raw = await queryBackend(query_type, query, limit);
      return formatResult(query_type, raw, query, limit);
    } catch (error) {
      return {
        query_type,
        error: error instanceof Error ? error.message : "Unknown error querying backend",
        hint: "The Rust backend may not be running. Start it with `cargo run -p stupid-server`.",
      };
    }
  },
});
