// ── Queue connection management — thin client over Rust backend ────

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

// ── Types ──────────────────────────────────────────────────────────

export interface QueueConnectionConfig {
  id: string;
  name: string;
  queue_url: string;
  dlq_url: string | null;
  provider: string;
  enabled: boolean;
  region: string;
  access_key_id: string;
  secret_access_key: string;
  session_token: string;
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

export type QueueConnectionInput = Omit<
  QueueConnectionConfig,
  "id" | "created_at" | "updated_at"
>;

export type QueueConnectionSafe = Omit<
  QueueConnectionConfig,
  "access_key_id" | "secret_access_key" | "session_token"
> & {
  access_key_id: "********";
  secret_access_key: "********";
  session_token: "********";
};

// ── CRUD ──────────────────────────────────────────────────────────

export async function listQueueConnections(): Promise<QueueConnectionSafe[]> {
  const res = await fetch(`${API_BASE}/queue-connections`, { cache: "no-store" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function getQueueConnection(
  id: string,
): Promise<QueueConnectionConfig | null> {
  const res = await fetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}/credentials`,
    { cache: "no-store" },
  );
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function addQueueConnection(
  input: Partial<QueueConnectionInput>,
): Promise<QueueConnectionSafe> {
  const res = await fetch(`${API_BASE}/queue-connections`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function updateQueueConnection(
  id: string,
  input: Partial<QueueConnectionInput>,
): Promise<QueueConnectionSafe | null> {
  const res = await fetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(input),
    },
  );
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function deleteQueueConnection(id: string): Promise<boolean> {
  const res = await fetch(
    `${API_BASE}/queue-connections/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
  if (res.status === 404) return false;
  if (!res.ok) throw new Error(await res.text());
  return true;
}
