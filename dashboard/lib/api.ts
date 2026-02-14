const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

export interface Stats {
  doc_count: number;
  segment_count: number;
  segment_ids: string[];
  node_count: number;
  edge_count: number;
  nodes_by_type: Record<string, number>;
  edges_by_type: Record<string, number>;
}

export interface ForceNode {
  id: string;
  entity_type: string;
  key: string;
}

export interface ForceLink {
  source: string;
  target: string;
  edge_type: string;
  weight: number;
}

export interface ForceGraphData {
  nodes: ForceNode[];
  links: ForceLink[];
}

export interface NodeDetail {
  id: string;
  entity_type: string;
  key: string;
  neighbors: {
    node_id: string;
    entity_type: string;
    key: string;
    edge_type: string;
    weight: number;
  }[];
}

export async function fetchStats(): Promise<Stats> {
  const res = await fetch(`${API_BASE}/stats`, { cache: "no-store" });
  return res.json();
}

export async function fetchForceGraph(limit = 300): Promise<ForceGraphData> {
  const res = await fetch(`${API_BASE}/graph/force?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchNodeDetail(id: string): Promise<NodeDetail> {
  const res = await fetch(`${API_BASE}/graph/nodes/${id}`, {
    cache: "no-store",
  });
  return res.json();
}

// Compute endpoints

export interface PageRankEntry {
  id: string;
  entity_type: string;
  key: string;
  score: number;
}

export interface CommunityNode {
  id: string;
  entity_type: string;
  key: string;
}

export interface CommunityEntry {
  community_id: number;
  member_count: number;
  top_nodes: CommunityNode[];
}

export interface DegreeEntry {
  id: string;
  entity_type: string;
  key: string;
  in_deg: number;
  out_deg: number;
  total: number;
}

export async function fetchPageRank(limit = 50): Promise<PageRankEntry[]> {
  const res = await fetch(`${API_BASE}/compute/pagerank?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchCommunities(): Promise<CommunityEntry[]> {
  const res = await fetch(`${API_BASE}/compute/communities`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchDegrees(limit = 50): Promise<DegreeEntry[]> {
  const res = await fetch(`${API_BASE}/compute/degrees?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

// Query endpoint

export interface QueryResponse {
  question: string;
  plan: unknown;
  results: unknown[];
}

export async function postQuery(question: string): Promise<QueryResponse> {
  const res = await fetch(`${API_BASE}/query`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ question }),
  });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(text || `Query failed (${res.status})`);
  }
  return res.json();
}
