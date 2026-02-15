/**
 * Catalog API client — fetches catalog data from the Rust backend.
 *
 * The catalog is the system's self-awareness layer: entity types, edge types,
 * external SQL sources, and aggregated compute/stats data.
 *
 * Calls Rust backend directly (same pattern as lib/api.ts).
 */

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

async function checkedFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `Request failed (${res.status})`);
  }
  return res;
}

// ── Catalog types (mirrors Rust crates/catalog/src/catalog.rs) ──

export interface CatalogEntry {
  entity_type: string;
  node_count: number;
  sample_keys: string[];
}

export interface EdgeSummary {
  edge_type: string;
  count: number;
  source_types: string[];
  target_types: string[];
}

export interface ExternalColumn {
  name: string;
  data_type: string;
}

export interface ExternalTable {
  name: string;
  columns: ExternalColumn[];
}

export interface ExternalDatabase {
  name: string;
  tables: ExternalTable[];
}

export interface ExternalSource {
  name: string;
  kind: string;
  connection_id: string;
  databases: ExternalDatabase[];
}

export interface Catalog {
  entity_types: CatalogEntry[];
  edge_types: EdgeSummary[];
  total_nodes: number;
  total_edges: number;
  external_sources?: ExternalSource[];
}

// ── Fetch functions ──

export async function fetchCatalog(): Promise<Catalog> {
  const res = await checkedFetch(`${API_BASE}/catalog`, { cache: "no-store" });
  return res.json();
}
