import type { Stats } from "@/lib/api";

/** Validate and extract Stats from a raw API response. */
export function adaptStatsData(raw: unknown): Stats {
  const r = raw as Record<string, unknown> | null | undefined;
  return {
    doc_count: (r?.doc_count as number) ?? 0,
    segment_count: (r?.segment_count as number) ?? 0,
    segment_ids: Array.isArray(r?.segment_ids) ? (r.segment_ids as string[]) : [],
    node_count: (r?.node_count as number) ?? 0,
    edge_count: (r?.edge_count as number) ?? 0,
    nodes_by_type: (r?.nodes_by_type as Record<string, number>) ?? {},
    edges_by_type: (r?.edges_by_type as Record<string, number>) ?? {},
  };
}
