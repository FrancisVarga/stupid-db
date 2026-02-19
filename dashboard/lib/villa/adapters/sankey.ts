import type { SankeyNodeDatum, SankeyLinkDatum } from "@/components/viz/SankeyDiagram";

interface SankeyPayload {
  nodes: SankeyNodeDatum[];
  links: SankeyLinkDatum[];
}

/**
 * Transform raw API data into Sankey nodes + links.
 *
 * Expects: { nodes: [{ id, label }], links: [{ source, target, value }] }
 */
export function adaptSankeyData(raw: unknown): SankeyPayload {
  const empty: SankeyPayload = { nodes: [], links: [] };

  if (!raw || typeof raw !== "object") return empty;
  const obj = raw as Record<string, unknown>;

  const rawNodes = Array.isArray(obj.nodes) ? obj.nodes : [];
  const rawLinks = Array.isArray(obj.links) ? obj.links : Array.isArray(obj.edges) ? obj.edges : [];

  const nodes: SankeyNodeDatum[] = rawNodes
    .filter((n): n is Record<string, unknown> => n !== null && typeof n === "object")
    .map((n) => ({
      id: String(n.id ?? ""),
      label: String(n.label ?? n.name ?? n.id ?? ""),
    }))
    .filter((n) => n.id);

  const nodeIds = new Set(nodes.map((n) => n.id));

  const links: SankeyLinkDatum[] = rawLinks
    .filter((l): l is Record<string, unknown> => l !== null && typeof l === "object")
    .map((l) => ({
      source: String(l.source ?? l.from ?? ""),
      target: String(l.target ?? l.to ?? ""),
      value: Number(l.value ?? l.weight ?? l.count ?? 1),
    }))
    .filter((l) => nodeIds.has(l.source) && nodeIds.has(l.target) && l.value > 0);

  return { nodes, links };
}
