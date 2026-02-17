// ── Generic Rules API Client ─────────────────────────────────────────
// Typed API client for all rule kinds (AnomalyRule, EntitySchema,
// FeatureConfig, ScoringConfig, TrendConfig, PatternConfig).
// Complements api-anomaly-rules.ts which keeps anomaly lifecycle ops.

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

async function checkedFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `Request failed (${res.status})`);
  }
  return res;
}

// ── Types ────────────────────────────────────────────────────────────

export type RuleKind =
  | "AnomalyRule"
  | "EntitySchema"
  | "FeatureConfig"
  | "ScoringConfig"
  | "TrendConfig"
  | "PatternConfig";

/** Lightweight summary returned by GET /rules */
export interface GenericRuleSummary {
  id: string;
  name: string;
  kind: RuleKind;
  enabled: boolean;
  description?: string;
  tags?: string[];
}

/** Full rule document — shape varies by kind.
 * All kinds share: apiVersion, kind, metadata.
 * AnomalyRule has: schedule, detection, filters, notifications.
 * Config kinds have: spec (kind-specific object). */
export interface RuleDocument {
  apiVersion: string;
  kind: string;
  metadata: {
    id: string;
    name: string;
    description?: string;
    tags?: string[];
    enabled: boolean;
    extends?: string;
  };
  // AnomalyRule fields
  schedule?: {
    cron: string;
    timezone?: string;
    cooldown?: string;
  };
  detection?: {
    template?: string;
    params?: Record<string, unknown>;
    compose?: unknown;
  };
  filters?: {
    entity_types?: string[];
    min_score?: number;
    exclude_keys?: string[];
  };
  notifications?: Array<{
    channel: string;
    [key: string]: unknown;
  }>;
  // Config kinds
  spec?: Record<string, unknown>;
}

// ── Kind metadata ────────────────────────────────────────────────────

export const RULE_KIND_META: Record<RuleKind, { label: string; color: string; short: string }> = {
  AnomalyRule: { label: "Anomaly Rule", color: "#f97316", short: "Anomaly" },
  EntitySchema: { label: "Entity Schema", color: "#06b6d4", short: "Entity" },
  FeatureConfig: { label: "Feature Config", color: "#a855f7", short: "Feature" },
  ScoringConfig: { label: "Scoring Config", color: "#10b981", short: "Scoring" },
  TrendConfig: { label: "Trend Config", color: "#3b82f6", short: "Trend" },
  PatternConfig: { label: "Pattern Config", color: "#eab308", short: "Pattern" },
};

export const ALL_RULE_KINDS: RuleKind[] = [
  "AnomalyRule",
  "EntitySchema",
  "FeatureConfig",
  "ScoringConfig",
  "TrendConfig",
  "PatternConfig",
];

// ── Dashboard Types ──────────────────────────────────────────────────

/** Match summary within a trigger entry. */
export interface RecentMatchSummary {
  entity_key: string;
  entity_type: string;
  score: number;
  reason: string;
}

/** A recent trigger entry enriched with rule metadata for the dashboard feed. */
export interface RecentTrigger {
  rule_id: string;
  rule_name: string;
  kind: RuleKind;
  timestamp: string;
  matches_found: number;
  evaluation_ms: number;
  matches?: RecentMatchSummary[];
}

// ── Dashboard Operations ─────────────────────────────────────────────

export async function getRecentTriggers(limit?: number): Promise<RecentTrigger[]> {
  const params = limit != null ? `?limit=${limit}` : "";
  const res = await checkedFetch(`${API_BASE}/rules/recent-triggers${params}`, { cache: "no-store" });
  return res.json();
}

// ── CRUD Operations ──────────────────────────────────────────────────

export async function listRules(kind?: RuleKind): Promise<GenericRuleSummary[]> {
  const params = kind ? `?kind=${encodeURIComponent(kind)}` : "";
  const res = await checkedFetch(`${API_BASE}/rules${params}`, { cache: "no-store" });
  return res.json();
}

export async function getRule(id: string): Promise<RuleDocument> {
  const res = await checkedFetch(`${API_BASE}/rules/${encodeURIComponent(id)}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function getRuleYaml(id: string): Promise<string> {
  const res = await checkedFetch(`${API_BASE}/rules/${encodeURIComponent(id)}/yaml`, {
    cache: "no-store",
  });
  return res.text();
}

export async function createRule(yamlContent: string): Promise<RuleDocument> {
  const res = await checkedFetch(`${API_BASE}/rules`, {
    method: "POST",
    headers: { "Content-Type": "text/plain" },
    body: yamlContent,
  });
  return res.json();
}

export async function updateRule(id: string, yamlContent: string): Promise<RuleDocument> {
  const res = await checkedFetch(`${API_BASE}/rules/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: { "Content-Type": "text/plain" },
    body: yamlContent,
  });
  return res.json();
}

export async function deleteRule(id: string): Promise<void> {
  await checkedFetch(`${API_BASE}/rules/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function toggleRule(id: string): Promise<RuleDocument> {
  const res = await checkedFetch(`${API_BASE}/rules/${encodeURIComponent(id)}/toggle`, {
    method: "POST",
  });
  return res.json();
}
