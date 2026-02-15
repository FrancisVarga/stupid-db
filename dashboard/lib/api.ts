const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";
export const WS_URL = API_BASE.replace(/^http/, "ws") + "/ws";

async function checkedFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `Request failed (${res.status})`);
  }
  return res;
}

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
  const res = await checkedFetch(`${API_BASE}/stats`, { cache: "no-store" });
  return res.json();
}

export async function fetchForceGraph(limit = 300): Promise<ForceGraphData> {
  const res = await checkedFetch(`${API_BASE}/graph/force?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchNodeDetail(id: string): Promise<NodeDetail> {
  const res = await checkedFetch(`${API_BASE}/graph/nodes/${id}`, {
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
  const res = await checkedFetch(`${API_BASE}/compute/pagerank?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchCommunities(): Promise<CommunityEntry[]> {
  const res = await checkedFetch(`${API_BASE}/compute/communities`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchDegrees(limit = 50): Promise<DegreeEntry[]> {
  const res = await checkedFetch(`${API_BASE}/compute/degrees?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

// Pattern detection endpoints

export interface TemporalPattern {
  id: string;
  sequence: string[];
  support: number;
  member_count: number;
  avg_duration_secs: number;
  category: "Churn" | "Engagement" | "ErrorChain" | "Funnel" | "Unknown";
  description: string | null;
}

export interface CooccurrenceEntry {
  entity_a: string;
  entity_b: string;
  count: number;
  pmi: number | null;
}

export interface CooccurrenceData {
  entity_type_a: string;
  entity_type_b: string;
  pairs: CooccurrenceEntry[];
}

export interface TrendEntry {
  metric: string;
  current_value: number;
  baseline_mean: number;
  direction: string;
  magnitude: number;
}

export async function fetchPatterns(): Promise<TemporalPattern[]> {
  const res = await checkedFetch(`${API_BASE}/compute/patterns`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchCooccurrence(
  typeA?: string,
  typeB?: string
): Promise<CooccurrenceData> {
  const params = new URLSearchParams();
  if (typeA) params.set("entity_type_a", typeA);
  if (typeB) params.set("entity_type_b", typeB);
  const qs = params.toString();
  const res = await checkedFetch(
    `${API_BASE}/compute/cooccurrence${qs ? `?${qs}` : ""}`,
    { cache: "no-store" }
  );
  // Backend returns Vec<CooccurrenceResponse> — take the first match or return empty.
  const responses: CooccurrenceData[] = await res.json();
  if (responses.length > 0) {
    return responses[0];
  }
  return { entity_type_a: typeA || "", entity_type_b: typeB || "", pairs: [] };
}

export async function fetchTrends(): Promise<TrendEntry[]> {
  const res = await checkedFetch(`${API_BASE}/compute/trends`, {
    cache: "no-store",
  });
  return res.json();
}

// Anomaly detection endpoints

export interface FeatureDimension {
  name: string;
  value: number;
}

export interface AnomalyEntry {
  id: string;
  entity_type: string;
  key: string;
  score: number;
  is_anomalous: boolean;
  features?: FeatureDimension[];
  cluster_id?: number;
}

export async function fetchAnomalies(limit = 50): Promise<AnomalyEntry[]> {
  const res = await checkedFetch(`${API_BASE}/compute/anomalies?limit=${limit}`, {
    cache: "no-store",
  });
  return res.json();
}

// Queue status endpoint

export interface QueueStatus {
  enabled: boolean;
  connected?: boolean;
  messages_received?: number;
  messages_processed?: number;
  messages_failed?: number;
  batches_processed?: number;
  avg_batch_latency_ms?: number;
  last_poll_epoch_ms?: number;
}

export async function fetchQueueStatus(): Promise<QueueStatus> {
  const res = await checkedFetch(`${API_BASE}/queue/status`, {
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
  const res = await checkedFetch(`${API_BASE}/query`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ question }),
  });
  return res.json();
}
