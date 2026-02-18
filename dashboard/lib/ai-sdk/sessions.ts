// ── AI SDK session API client ─────────────────────────────────────────

export interface AiSdkSession {
  id: string;
  title: string;
  provider: string;
  model: string;
  created_at: string;
  updated_at: string;
}

export interface AiSdkMessage {
  id: string;
  session_id: string;
  role: string;
  content: unknown; // JSONB — UIMessage parts array
  metadata: unknown; // JSONB
  created_at: string;
}

export interface AiSdkSessionWithMessages extends AiSdkSession {
  messages: AiSdkMessage[];
}

const BASE = "/api/ai-sdk/sessions";

export async function listSessions(): Promise<AiSdkSession[]> {
  const res = await fetch(BASE, { cache: "no-store" });
  if (!res.ok) throw new Error(`Failed to list sessions: ${res.status}`);
  return res.json();
}

export async function createSession(opts?: {
  title?: string;
  provider?: string;
  model?: string;
}): Promise<AiSdkSession> {
  const res = await fetch(BASE, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(opts ?? {}),
  });
  if (!res.ok) throw new Error(`Failed to create session: ${res.status}`);
  return res.json();
}

export async function getSession(
  id: string,
  opts?: { limit?: number; offset?: number },
): Promise<AiSdkSessionWithMessages> {
  const params = new URLSearchParams();
  if (opts?.limit) params.set("limit", String(opts.limit));
  if (opts?.offset) params.set("offset", String(opts.offset));
  const qs = params.toString();
  const res = await fetch(`${BASE}/${id}${qs ? `?${qs}` : ""}`, {
    cache: "no-store",
  });
  if (!res.ok) throw new Error(`Failed to get session: ${res.status}`);
  return res.json();
}

export async function updateSession(
  id: string,
  updates: { title?: string; provider?: string; model?: string },
): Promise<AiSdkSession> {
  const res = await fetch(`${BASE}/${id}`, {
    method: "PATCH",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(updates),
  });
  if (!res.ok) throw new Error(`Failed to update session: ${res.status}`);
  return res.json();
}

export async function deleteSession(id: string): Promise<void> {
  const res = await fetch(`${BASE}/${id}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Failed to delete session: ${res.status}`);
}
