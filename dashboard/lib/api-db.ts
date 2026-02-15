// ── PG Manager API Client ─────────────────────────────────────────────
// Same checkedFetch pattern as lib/api.ts but targets /api/v1/* routes
// on the same origin (Next.js API routes proxying to Postgres).

async function checkedFetch(url: string, init?: RequestInit): Promise<Response> {
  const res = await fetch(url, init);
  if (!res.ok) {
    const text = await res.text().catch(() => "");
    throw new Error(text || `Request failed (${res.status})`);
  }
  return res;
}

// ── Types ─────────────────────────────────────────────────────────────

export interface ConnectionInput {
  name: string;
  connection_string?: string;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  ssl: boolean;
  color: string;
}

export interface ConnectionSafe {
  id: string;
  name: string;
  host: string;
  port: number;
  database: string;
  username: string;
  password: string; // always "********"
  ssl: boolean;
  color: string;
  created_at: string;
  updated_at: string;
}

export interface Database extends ConnectionSafe {
  table_count: number;
  size: string;
  status: "connected" | "error";
  error?: string;
}

export interface TestConnectionResult {
  ok: boolean;
  version?: string;
  database?: string;
  duration_ms?: number;
  error?: string;
}

// ── Connection Management ─────────────────────────────────────────────

export async function fetchConnections(): Promise<ConnectionSafe[]> {
  const res = await checkedFetch("/api/v1/meta/connections", { cache: "no-store" });
  return res.json();
}

export async function fetchConnection(id: string): Promise<ConnectionSafe> {
  const res = await checkedFetch(`/api/v1/meta/connections/${encodeURIComponent(id)}`, {
    cache: "no-store",
  });
  return res.json();
}

export async function addConnectionApi(input: ConnectionInput): Promise<ConnectionSafe> {
  const res = await checkedFetch("/api/v1/meta/connections", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  return res.json();
}

export async function updateConnectionApi(
  id: string,
  input: Partial<ConnectionInput>,
): Promise<ConnectionSafe> {
  const res = await checkedFetch(`/api/v1/meta/connections/${encodeURIComponent(id)}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  return res.json();
}

export async function deleteConnectionApi(id: string): Promise<void> {
  await checkedFetch(`/api/v1/meta/connections/${encodeURIComponent(id)}`, {
    method: "DELETE",
  });
}

export async function testConnection(
  input: Partial<ConnectionInput>,
): Promise<TestConnectionResult> {
  const res = await fetch("/api/v1/meta/connections/test", {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(input),
  });
  return res.json();
}

export interface Table {
  schema: string;
  name: string;
  estimated_rows: number;
  size: string;
  has_pk: boolean;
}

export interface Column {
  name: string;
  type: string;
  udt_name: string;
  nullable: boolean;
  default_value: string | null;
  is_pk: boolean;
  is_unique: boolean;
  is_indexed: boolean;
  fk_target: string | null;
}

export interface PaginatedResponse<T = Record<string, unknown>> {
  rows: T[];
  total: number;
  page: number;
  limit: number;
  columns?: Column[];
}

export interface AuditEntry {
  id: number;
  timestamp: string;
  method: string;
  path: string;
  table_name: string | null;
  operation: string;
  record_id: string | null;
  record_ids: string[] | null;
  request_body: unknown;
  response_status: number;
  row_count: number | null;
  duration_ms: number;
  sql_executed: string | null;
  error: string | null;
  ip: string | null;
  user_agent: string | null;
}

export interface AuditStats {
  by_table: { table_name: string; count: number }[];
  by_operation: { operation: string; count: number }[];
  by_hour: { hour: string; count: number }[];
  error_rate: number;
  avg_duration_ms: number;
  slowest_queries: { sql: string; duration_ms: number; timestamp: string }[];
}

export interface QueryResult {
  columns: string[];
  rows: Record<string, unknown>[];
  row_count: number;
  duration_ms: number;
}

// ── Metadata Endpoints ────────────────────────────────────────────────

export async function fetchDatabases(): Promise<Database[]> {
  const res = await checkedFetch("/api/v1/meta/databases", {
    cache: "no-store",
  });
  return res.json();
}

export async function fetchTables(db: string): Promise<Table[]> {
  const res = await checkedFetch(
    `/api/v1/meta/${encodeURIComponent(db)}/tables`,
    { cache: "no-store" }
  );
  return res.json();
}

export async function fetchTableSchema(
  db: string,
  table: string
): Promise<Column[]> {
  const res = await checkedFetch(
    `/api/v1/meta/${encodeURIComponent(db)}/${encodeURIComponent(table)}/schema`,
    { cache: "no-store" }
  );
  return res.json();
}

// ── CRUD Endpoints ────────────────────────────────────────────────────

export interface FetchRowsParams {
  page?: number;
  limit?: number;
  sort?: string;
  order?: "asc" | "desc";
  filter?: Record<string, unknown>;
}

export async function fetchRows(
  db: string,
  table: string,
  params: FetchRowsParams = {}
): Promise<PaginatedResponse> {
  const qs = new URLSearchParams();
  if (params.page) qs.set("page", String(params.page));
  if (params.limit) qs.set("limit", String(params.limit));
  if (params.sort) qs.set("sort", params.sort);
  if (params.order) qs.set("order", params.order);
  if (params.filter && Object.keys(params.filter).length > 0) {
    qs.set("filter", JSON.stringify(params.filter));
  }
  const query = qs.toString();
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}${query ? `?${query}` : ""}`,
    { cache: "no-store" }
  );
  return res.json();
}

export async function fetchRow(
  db: string,
  table: string,
  id: string
): Promise<Record<string, unknown>> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}/${encodeURIComponent(id)}`,
    { cache: "no-store" }
  );
  return res.json();
}

export async function createRow(
  db: string,
  table: string,
  data: Record<string, unknown>
): Promise<Record<string, unknown>> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    }
  );
  return res.json();
}

export async function updateRow(
  db: string,
  table: string,
  id: string,
  data: Record<string, unknown>
): Promise<Record<string, unknown>> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}/${encodeURIComponent(id)}`,
    {
      method: "PUT",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(data),
    }
  );
  return res.json();
}

export async function deleteRow(
  db: string,
  table: string,
  id: string
): Promise<void> {
  await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}/${encodeURIComponent(id)}`,
    { method: "DELETE" }
  );
}

// ── Batch Operations ──────────────────────────────────────────────────

export async function batchUpdate(
  db: string,
  table: string,
  ids: string[],
  updates: Record<string, unknown>
): Promise<{ updated: number }> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}`,
    {
      method: "PATCH",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ids, updates }),
    }
  );
  return res.json();
}

export async function batchDelete(
  db: string,
  table: string,
  ids: string[]
): Promise<{ deleted: number }> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/${encodeURIComponent(table)}`,
    {
      method: "DELETE",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ ids }),
    }
  );
  return res.json();
}

// ── Raw SQL Query ─────────────────────────────────────────────────────

export async function executeQuery(
  db: string,
  sql: string,
  params: unknown[] = []
): Promise<QueryResult> {
  const res = await checkedFetch(
    `/api/v1/${encodeURIComponent(db)}/query`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sql, params }),
    }
  );
  return res.json();
}

// ── Audit Endpoints ───────────────────────────────────────────────────

export interface FetchAuditParams {
  table?: string;
  operation?: string;
  page?: number;
  limit?: number;
}

export async function fetchAuditLog(
  db: string,
  params: FetchAuditParams = {}
): Promise<PaginatedResponse<AuditEntry>> {
  const qs = new URLSearchParams();
  if (params.table) qs.set("table", params.table);
  if (params.operation) qs.set("operation", params.operation);
  if (params.page) qs.set("page", String(params.page));
  if (params.limit) qs.set("limit", String(params.limit));
  const query = qs.toString();
  const res = await checkedFetch(
    `/api/v1/meta/${encodeURIComponent(db)}/audit${query ? `?${query}` : ""}`,
    { cache: "no-store" }
  );
  return res.json();
}

export async function fetchAuditStats(db: string): Promise<AuditStats> {
  const res = await checkedFetch(
    `/api/v1/meta/${encodeURIComponent(db)}/audit/stats`,
    { cache: "no-store" }
  );
  return res.json();
}
