// ── Anomaly Rules API Client ─────────────────────────────────────────
// Typed API client for anomaly rule CRUD and lifecycle operations.
// Follows the same checkedFetch pattern as lib/api.ts.

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

async function checkedFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `Request failed (${res.status})`);
  }
  return res;
}

// ── Types (matching Rust API response types) ─────────────────────────

/** Lightweight summary returned by GET /anomaly-rules */
export interface RuleSummary {
  id: string;
  name: string;
  enabled: boolean;
  template: string | null;
  cron: string;
  channel_count: number;
  last_triggered?: string | null;
  trigger_count: number;
}

/** Full anomaly rule (returned by GET/POST/PUT endpoints) */
export interface AnomalyRule {
  apiVersion: string;
  kind: string;
  metadata: RuleMetadata;
  schedule: Schedule;
  detection: Detection;
  filters?: Filters;
  notifications: NotificationChannel[];
}

export interface RuleMetadata {
  id: string;
  name: string;
  description?: string;
  enabled: boolean;
  tags?: string[];
}

export interface Schedule {
  cron: string;
  timezone?: string;
  cooldown?: string;
}

export interface Detection {
  template?: string;
  params?: Record<string, unknown>;
  compose?: Composition;
  enrich?: Record<string, unknown>;
}

export interface Composition {
  operator: "and" | "or" | "not";
  conditions: Condition[];
}

export interface Condition {
  signal?: SignalCondition;
  compose?: Composition;
}

export interface SignalCondition {
  type: string;
  feature?: string;
  threshold: number;
  operator?: string;
}

export interface Filters {
  entity_types?: string[];
  min_score?: number;
  exclude_keys?: string[];
  where_conditions?: FilterCondition[];
}

export interface FilterCondition {
  feature: string;
  operator: string;
  value: number;
}

export interface NotificationChannel {
  channel: "webhook" | "email" | "telegram";
  on?: string[];
  // Webhook fields
  url?: string;
  method?: string;
  headers?: Record<string, string>;
  body_template?: string;
  // Email fields
  smtp_host?: string;
  smtp_port?: number;
  tls?: boolean;
  from?: string;
  to?: string[];
  subject?: string;
  template?: string;
  // Telegram fields
  bot_token?: string;
  chat_id?: string;
  parse_mode?: string;
}

// ── Lifecycle response types ─────────────────────────────────────────

/** Result of POST /anomaly-rules/{id}/run */
export interface RunResult {
  rule_id: string;
  matches_found: number;
  evaluation_ms: number;
  message: string;
}

/** Result of POST /anomaly-rules/{id}/test-notify */
export interface TestNotifyResult {
  channel: string;
  success: boolean;
  error: string | null;
  response_ms: number;
}

/** Single trigger history entry from GET /anomaly-rules/{id}/history */
export interface TriggerEntry {
  timestamp: string;
  matches_found: number;
  evaluation_ms: number;
}

// ── CRUD Operations ──────────────────────────────────────────────────

export async function listAnomalyRules(): Promise<RuleSummary[]> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules`, { cache: "no-store" });
  return res.json();
}

export async function getAnomalyRule(id: string): Promise<AnomalyRule> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function createAnomalyRule(yamlContent: string): Promise<AnomalyRule> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules`, {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: yamlContent,
  });
  return res.json();
}

export async function updateAnomalyRule(
  id: string,
  yamlContent: string,
): Promise<AnomalyRule> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: { "Content-Type": "text/plain" },
    body: yamlContent,
  });
  return res.json();
}

export async function deleteAnomalyRule(id: string): Promise<void> {
  await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

// ── Lifecycle Operations ─────────────────────────────────────────────

export async function startRule(id: string): Promise<AnomalyRule> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/start`, {
    method: "POST",
  });
  return res.json();
}

export async function pauseRule(id: string): Promise<AnomalyRule> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/pause`, {
    method: "POST",
  });
  return res.json();
}

export async function runRuleNow(id: string): Promise<RunResult> {
  const res = await checkedFetch(`${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/run`, {
    method: "POST",
  });
  return res.json();
}

export async function testNotify(id: string): Promise<TestNotifyResult[]> {
  const res = await checkedFetch(
    `${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/test-notify`,
    { method: "POST" },
  );
  return res.json();
}

export async function getRuleHistory(
  id: string,
  limit?: number,
): Promise<TriggerEntry[]> {
  const params = limit != null ? `?limit=${limit}` : "";
  const res = await checkedFetch(
    `${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/history${params}`,
    { cache: "no-store" },
  );
  return res.json();
}

// ── Audit Log Types ─────────────────────────────────────────────────

export type AuditLogLevel = "debug" | "info" | "warning" | "error";

export type AuditExecutionPhase =
  | "schedule_check"
  | "evaluation"
  | "template_match"
  | "signal_check"
  | "filter_apply"
  | "enrichment"
  | "rate_limit"
  | "notification"
  | "notify_error"
  | "complete";

export interface AuditLogEntry {
  timestamp: string;
  rule_id: string;
  level: AuditLogLevel;
  phase: AuditExecutionPhase;
  message: string;
  details?: unknown;
  duration_ms?: number;
}

// ── Audit Log Operations ────────────────────────────────────────────

export async function getRuleLogs(
  id: string,
  params?: {
    level?: AuditLogLevel;
    phase?: AuditExecutionPhase;
    limit?: number;
    since?: string;
  },
): Promise<AuditLogEntry[]> {
  const searchParams = new URLSearchParams();
  if (params?.level) searchParams.set("level", params.level);
  if (params?.phase) searchParams.set("phase", params.phase);
  if (params?.limit != null) searchParams.set("limit", String(params.limit));
  if (params?.since) searchParams.set("since", params.since);
  const qs = searchParams.toString();
  const url = `${API_BASE}/anomaly-rules/${encodeURIComponent(id)}/logs${qs ? `?${qs}` : ""}`;
  const res = await checkedFetch(url, { cache: "no-store" });
  return res.json();
}
