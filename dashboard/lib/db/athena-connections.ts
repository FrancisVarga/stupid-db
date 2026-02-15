// ── Athena connection management — thin client over Rust backend ────

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

// ── Types ──────────────────────────────────────────────────────────

export interface AthenaSchema {
  databases: AthenaDatabase[];
  fetched_at: string;
}

export interface AthenaDatabase {
  name: string;
  tables: AthenaTable[];
}

export interface AthenaTable {
  name: string;
  columns: AthenaColumn[];
}

export interface AthenaColumn {
  name: string;
  data_type: string;
  comment: string | null;
}

export interface AthenaConnectionConfig {
  id: string;
  name: string;
  region: string;
  catalog: string;
  database: string;
  workgroup: string;
  output_location: string;
  access_key_id: string;
  secret_access_key: string;
  session_token: string;
  endpoint_url: string | null;
  enabled: boolean;
  color: string;
  schema: AthenaSchema | null;
  schema_status: string;
  created_at: string;
  updated_at: string;
}

/** What the user sends (credentials in plain text). */
export type AthenaConnectionInput = Omit<
  AthenaConnectionConfig,
  "id" | "created_at" | "updated_at" | "schema" | "schema_status"
>;

/** What we return to the client (credentials masked). */
export type AthenaConnectionSafe = Omit<
  AthenaConnectionConfig,
  "access_key_id" | "secret_access_key" | "session_token"
> & {
  access_key_id: "********";
  secret_access_key: "********";
  session_token: "********";
};

// ── CRUD ──────────────────────────────────────────────────────────

export async function listAthenaConnections(): Promise<AthenaConnectionSafe[]> {
  const res = await fetch(`${API_BASE}/athena-connections`, { cache: "no-store" });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function getAthenaConnection(
  id: string,
): Promise<AthenaConnectionConfig | null> {
  const res = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(id)}/credentials`,
    { cache: "no-store" },
  );
  if (res.status === 404) return null;
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function addAthenaConnection(
  input: Partial<AthenaConnectionInput>,
): Promise<AthenaConnectionSafe> {
  const res = await fetch(`${API_BASE}/athena-connections`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function updateAthenaConnection(
  id: string,
  input: Partial<AthenaConnectionInput>,
): Promise<AthenaConnectionSafe | null> {
  const res = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(id)}`,
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

export async function deleteAthenaConnection(id: string): Promise<boolean> {
  const res = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(id)}`,
    { method: "DELETE" },
  );
  if (res.status === 404) return false;
  if (!res.ok) throw new Error(await res.text());
  return true;
}

// ── Schema ────────────────────────────────────────────────────────

export interface SchemaResponse {
  schema_status: string;
  schema: AthenaSchema | null;
}

export async function getAthenaSchema(id: string): Promise<SchemaResponse> {
  const res = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(id)}/schema`,
    { cache: "no-store" },
  );
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}

export async function refreshAthenaSchema(
  id: string,
): Promise<{ status: string; message: string }> {
  const res = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(id)}/schema/refresh`,
    { method: "POST" },
  );
  if (!res.ok) throw new Error(await res.text());
  return res.json();
}
