// ── Connection management — thin client over Rust backend ────────

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

// ── Types ──────────────────────────────────────────────────────────

export interface ConnectionConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  ssl: boolean;
  color: string;
  created_at: string;
  updated_at: string;
}

/** What the user sends (password in plain text). */
export type ConnectionInput = Omit<ConnectionConfig, "id" | "created_at" | "updated_at"> & {
  connection_string?: string;
};

/** What we return to the client (password masked). */
export type ConnectionSafe = Omit<ConnectionConfig, "password"> & { password: "********" };

// ── CRUD ──────────────────────────────────────────────────────────

export async function listConnections(): Promise<ConnectionSafe[]> {
  const res = await fetch(`${API_BASE}/connections`, { cache: "no-store" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function getConnection(id: string): Promise<ConnectionConfig | null> {
  const res = await fetch(
    `${API_BASE}/connections/${encodeURIComponent(id)}/credentials`,
    { cache: "no-store" },
  );
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function addConnection(input: ConnectionInput): Promise<ConnectionSafe> {
  const res = await fetch(`${API_BASE}/connections`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function updateConnection(
  id: string,
  input: Partial<ConnectionInput>,
): Promise<ConnectionSafe | null> {
  const res = await fetch(
    `${API_BASE}/connections/${encodeURIComponent(id)}`,
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

export async function deleteConnection(id: string): Promise<boolean> {
  const res = await fetch(
    `${API_BASE}/connections/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
  if (res.status === 404) return false;
  if (!res.ok) throw new Error(await res.text());
  return true;
}
