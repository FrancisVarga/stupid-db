const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";
export const WS_URL = API_BASE.replace(/^http/, "ws") + "/ws";

async function checkedFetch(
  url: string,
  init?: RequestInit,
  timeoutMs = 60_000,
): Promise<Response> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), timeoutMs);

  try {
    const res = await fetch(url, { ...init, signal: controller.signal });
    if (!res.ok) {
      const text = await res.text().catch(() => "");
      throw new Error(text || `Request failed (${res.status})`);
    }
    return res;
  } catch (e: unknown) {
    if (e instanceof DOMException && e.name === "AbortError") {
      throw new Error(`Request timed out after ${timeoutMs / 1000}s — is the backend running at ${API_BASE}?`);
    }
    if (e instanceof TypeError && (e.message.includes("fetch") || e.message.includes("Failed"))) {
      throw new Error(`Cannot connect to backend at ${API_BASE} — is the server running?`);
    }
    throw e;
  } finally {
    clearTimeout(timer);
  }
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

export interface QueueMetricsEntry {
  enabled: boolean;
  connected: boolean;
  messages_received: number;
  messages_processed: number;
  messages_failed: number;
  batches_processed: number;
  avg_batch_latency_ms: number;
  last_poll_epoch_ms: number;
}

export interface QueueStatus {
  enabled: boolean;
  queues: Record<string, QueueMetricsEntry>;
}

export async function fetchQueueStatus(): Promise<QueueStatus> {
  const res = await checkedFetch(`${API_BASE}/queue/status`, {
    cache: "no-store",
  });
  return res.json();
}

// ── Queue Connection Management ──────────────────────────────────

export interface QueueConnectionInput {
  name: string;
  queue_url: string;
  dlq_url?: string;
  provider?: string;
  enabled?: boolean;
  region: string;
  access_key_id?: string;
  secret_access_key?: string;
  session_token?: string;
  endpoint_url?: string;
  poll_interval_ms?: number;
  max_batch_size?: number;
  visibility_timeout_secs?: number;
  micro_batch_size?: number;
  micro_batch_timeout_ms?: number;
  color?: string;
}

export interface QueueConnectionSafe {
  id: string;
  name: string;
  queue_url: string;
  dlq_url: string | null;
  provider: string;
  enabled: boolean;
  region: string;
  access_key_id: string;  // "********"
  secret_access_key: string;  // "********"
  session_token: string;  // "********"
  endpoint_url: string | null;
  poll_interval_ms: number;
  max_batch_size: number;
  visibility_timeout_secs: number;
  micro_batch_size: number;
  micro_batch_timeout_ms: number;
  color: string;
  created_at: string;
  updated_at: string;
}

export async function fetchQueueConnections(): Promise<QueueConnectionSafe[]> {
  const res = await checkedFetch(`${API_BASE}/queue-connections`, {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchQueueConnection(id: string): Promise<QueueConnectionSafe> {
  const res = await checkedFetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}`,
    { cache: "no-store" },
  );
  return res.json();
}

export async function addQueueConnectionApi(
  input: QueueConnectionInput,
): Promise<QueueConnectionSafe> {
  const res = await checkedFetch(`${API_BASE}/queue-connections`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  return res.json();
}

export async function updateQueueConnectionApi(
  id: string,
  input: Partial<QueueConnectionInput>,
): Promise<QueueConnectionSafe> {
  const res = await checkedFetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    },
  );
  return res.json();
}

export async function deleteQueueConnectionApi(id: string): Promise<void> {
  await checkedFetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
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

// Agent endpoints

export interface AgentInfo {
  name: string;
  tier: string;
  description: string;
}

export interface AgentResponse {
  agent_name: string;
  status: string;
  output: string;
  execution_time_ms: number;
}

export interface TeamResponse {
  task: string;
  strategy: string;
  agents_used: string[];
  status: string;
  outputs: Record<string, string>;
  execution_time_ms: number;
}

export async function fetchAgents(): Promise<AgentInfo[]> {
  const res = await checkedFetch(`${API_BASE}/agents/list`, {
    cache: "no-store",
  });
  const data = await res.json();
  return data.agents ?? [];
}

export async function executeAgent(
  agentName: string,
  task: string
): Promise<AgentResponse> {
  const res = await checkedFetch(`${API_BASE}/agents/execute`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ agent_name: agentName, task }),
  });
  return res.json();
}

export async function executeTeam(
  task: string,
  strategy: string
): Promise<TeamResponse> {
  const res = await checkedFetch(`${API_BASE}/teams/execute`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ task, strategy }),
  });
  return res.json();
}

export interface StrategyInfo {
  name: string;
  agents: string[];
  description: string;
}

export async function fetchStrategies(): Promise<StrategyInfo[]> {
  const res = await checkedFetch(`${API_BASE}/teams/strategies`, {
    cache: "no-store",
  });
  const data = await res.json();
  return data.strategies ?? [];
}

// ── Session Management ──────────────────────────────────────────

export interface SessionSummary {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  message_count: number;
  last_agent: string | null;
  last_mode: string | null;
}

export interface SessionMessage {
  id: string;
  role: "user" | "agent" | "team" | "error";
  content: string;
  timestamp: string;
  agent_name?: string;
  status?: string;
  execution_time_ms?: number;
  team_outputs?: Record<string, string>;
  agents_used?: string[];
  strategy?: string;
}

export interface Session {
  id: string;
  name: string;
  created_at: string;
  updated_at: string;
  messages: SessionMessage[];
  last_agent?: string;
  last_mode?: string;
}

export interface SessionExecuteResponse<T> {
  session: SessionSummary;
  response: T;
}

export async function fetchSessions(): Promise<SessionSummary[]> {
  const res = await checkedFetch(`${API_BASE}/sessions`, {
    cache: "no-store",
  });
  return res.json();
}

export async function createSession(
  name?: string
): Promise<Session> {
  const res = await checkedFetch(`${API_BASE}/sessions`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ name: name ?? null }),
  });
  return res.json();
}

export async function fetchSession(id: string): Promise<Session> {
  const res = await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(id)}`,
    { cache: "no-store" }
  );
  return res.json();
}

export async function renameSession(
  id: string,
  name: string
): Promise<Session> {
  const res = await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(id)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ name }),
    }
  );
  return res.json();
}

export async function deleteSession(id: string): Promise<void> {
  await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );
}

export async function executeAgentInSession(
  sessionId: string,
  agentName: string,
  task: string,
  maxHistory = 10
): Promise<SessionExecuteResponse<AgentResponse>> {
  const res = await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(sessionId)}/execute-agent`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        agent_name: agentName,
        task,
        max_history: maxHistory,
      }),
    }
  );
  return res.json();
}

export async function executeInSession(
  sessionId: string,
  task: string,
  maxHistory = 10
): Promise<SessionExecuteResponse<AgentResponse>> {
  const res = await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(sessionId)}/execute`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        task,
        max_history: maxHistory,
      }),
    }
  );
  return res.json();
}

export async function executeTeamInSession(
  sessionId: string,
  task: string,
  strategy: string,
  maxHistory = 10
): Promise<SessionExecuteResponse<TeamResponse>> {
  const res = await checkedFetch(
    `${API_BASE}/sessions/${encodeURIComponent(sessionId)}/execute-team`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        task,
        strategy,
        max_history: maxHistory,
      }),
    }
  );
  return res.json();
}

// ── Embeddings API ─────────────────────────────────

export interface EmbeddingDocument {
  id: string;
  filename: string;
  file_type: string;
  file_size: number;
  uploaded_at: string;
  chunk_count: number;
}

export interface SearchResult {
  chunk_id: string;
  document_id: string;
  filename: string;
  content: string;
  chunk_index: number;
  page_number: number | null;
  section_heading: string | null;
  similarity: number;
}

export async function uploadDocument(
  file: File
): Promise<{
  document_id: string;
  filename: string;
  chunk_count: number;
  file_size: number;
}> {
  const formData = new FormData();
  formData.append("file", file);
  // 10-minute timeout: scanned PDFs with OCR can take several minutes
  const res = await checkedFetch(
    `${API_BASE}/embeddings/upload`,
    { method: "POST", body: formData },
    600_000,
  );
  return res.json();
}

export async function searchEmbeddings(
  query: string,
  limit = 10
): Promise<{ results: SearchResult[] }> {
  const res = await checkedFetch(`${API_BASE}/embeddings/search`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ query, limit }),
  });
  return res.json();
}

export async function listEmbeddingDocuments(): Promise<{
  documents: EmbeddingDocument[];
}> {
  const res = await checkedFetch(`${API_BASE}/embeddings/documents`, {
    cache: "no-store",
  });
  return res.json();
}

export async function deleteEmbeddingDocument(id: string): Promise<void> {
  await checkedFetch(
    `${API_BASE}/embeddings/documents/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );
}
