import type { ForceGraphData, ForceNode, ForceLink } from "@/lib/api";

/**
 * Transform raw API response into ForceGraphData for the ForceGraph component.
 *
 * Handles two shapes:
 *  1. ForceGraphData — an object with `nodes` and `links` arrays (from /graph/force).
 *  2. An object with `nodes` and `edges` (alternative naming from some endpoints).
 *
 * Returns empty graph when data is missing or malformed.
 */
export function adaptForceGraphData(raw: unknown): ForceGraphData {
  const empty: ForceGraphData = { nodes: [], links: [] };

  if (!raw || typeof raw !== "object") return empty;

  const obj = raw as Record<string, unknown>;

  // Extract nodes
  const rawNodes = obj.nodes;
  if (!Array.isArray(rawNodes) || rawNodes.length === 0) return empty;

  const nodes: ForceNode[] = rawNodes.map((n: Record<string, unknown>) => ({
    id: String(n.id ?? ""),
    entity_type: String(n.entity_type ?? n.type ?? "Unknown"),
    key: String(n.key ?? n.label ?? n.id ?? ""),
  }));

  // Extract links — accept both "links" and "edges" keys
  const rawLinks = Array.isArray(obj.links)
    ? obj.links
    : Array.isArray(obj.edges)
      ? obj.edges
      : [];

  const nodeIds = new Set(nodes.map((n) => n.id));

  const links: ForceLink[] = (rawLinks as Record<string, unknown>[])
    .map((l) => ({
      source: String(l.source ?? l.from ?? ""),
      target: String(l.target ?? l.to ?? ""),
      edge_type: String(l.edge_type ?? l.type ?? "related"),
      weight: Number(l.weight ?? 1),
    }))
    .filter((l) => nodeIds.has(l.source) && nodeIds.has(l.target));

  return { nodes, links };
}
