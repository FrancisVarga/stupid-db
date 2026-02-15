import { NextRequest, NextResponse } from "next/server";
import { getPool } from "./client";

// ── Types ──────────────────────────────────────────────────────────

export interface AuditEntry {
  method: string;
  path: string;
  table_name?: string;
  operation: string;
  record_id?: string;
  record_ids?: string[];
  request_body?: unknown;
  response_status: number;
  row_count?: number;
  duration_ms: number;
  sql_executed?: string;
  error?: string;
  ip?: string;
  user_agent?: string;
}

export interface AuditContext {
  table?: string;
  operation: string;
  recordId?: string;
  recordIds?: string[];
  sqlExecuted?: string;
  rowCount?: number;
}

// ── Audit table creation ───────────────────────────────────────────

const initializedDbs = new Set<string>();

async function ensureAuditTable(dbName: string): Promise<void> {
  if (initializedDbs.has(dbName)) return;

  const sql = await getPool(dbName);
  await sql`
    CREATE TABLE IF NOT EXISTS _stupid_audit (
      id              BIGSERIAL PRIMARY KEY,
      timestamp       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
      method          TEXT NOT NULL,
      path            TEXT NOT NULL,
      table_name      TEXT,
      operation       TEXT NOT NULL,
      record_id       TEXT,
      record_ids      TEXT[],
      request_body    JSONB,
      response_status INT NOT NULL,
      row_count       INT,
      duration_ms     FLOAT NOT NULL,
      sql_executed    TEXT,
      error           TEXT,
      ip              TEXT,
      user_agent      TEXT
    )
  `;
  await sql`CREATE INDEX IF NOT EXISTS idx_audit_ts ON _stupid_audit(timestamp DESC)`;
  await sql`CREATE INDEX IF NOT EXISTS idx_audit_table ON _stupid_audit(table_name, operation)`;
  await sql`CREATE INDEX IF NOT EXISTS idx_audit_record ON _stupid_audit(table_name, record_id)`;

  initializedDbs.add(dbName);
}

// ── Console logging ────────────────────────────────────────────────

function logToConsole(db: string, entry: AuditEntry): void {
  const log = {
    timestamp: new Date().toISOString(),
    level: entry.error ? "error" : "info",
    db,
    method: entry.method,
    path: entry.path,
    table: entry.table_name,
    operation: entry.operation,
    duration_ms: Math.round(entry.duration_ms * 100) / 100,
    status: entry.response_status,
    row_count: entry.row_count,
    ...(entry.error ? { error: entry.error } : {}),
  };
  console.log(JSON.stringify(log));
}

// ── Audit write ────────────────────────────────────────────────────

async function writeAudit(dbName: string, entry: AuditEntry): Promise<void> {
  try {
    await ensureAuditTable(dbName);
    const sql = await getPool(dbName);
    await sql`
      INSERT INTO _stupid_audit (
        method, path, table_name, operation, record_id, record_ids,
        request_body, response_status, row_count, duration_ms,
        sql_executed, error, ip, user_agent
      ) VALUES (
        ${entry.method},
        ${entry.path},
        ${entry.table_name ?? null},
        ${entry.operation},
        ${entry.record_id ?? null},
        ${entry.record_ids ?? null},
        ${entry.request_body ? JSON.stringify(entry.request_body) : null},
        ${entry.response_status},
        ${entry.row_count ?? null},
        ${entry.duration_ms},
        ${entry.sql_executed ?? null},
        ${entry.error ?? null},
        ${entry.ip ?? null},
        ${entry.user_agent ?? null}
      )
    `;
  } catch (err) {
    // Never let audit failures break the main request
    console.error("Audit write failed:", err);
  }
}

// ── Middleware ──────────────────────────────────────────────────────

/**
 * Wrap an API route handler with audit logging.
 *
 * The handler receives a `setAuditContext` function it can call to enrich
 * the audit entry with operation-specific details (table, recordId, etc.).
 *
 * Usage:
 * ```ts
 * export async function GET(req: NextRequest, { params }: { params: Promise<{ db: string }> }) {
 *   const { db } = await params;
 *   return withAudit(db, req, (ctx) => {
 *     ctx.operation = "list";
 *     ctx.table = "users";
 *     // ... do work ...
 *     ctx.rowCount = rows.length;
 *     return NextResponse.json(rows);
 *   });
 * }
 * ```
 */
export async function withAudit(
  db: string,
  req: NextRequest,
  handler: (ctx: AuditContext) => Promise<Response>,
): Promise<Response> {
  const start = performance.now();
  const ctx: AuditContext = { operation: "unknown" };

  let body: unknown = undefined;
  if (req.method !== "GET" && req.method !== "HEAD") {
    try {
      body = await req.clone().json();
    } catch {
      // Body might not be JSON
    }
  }

  let response: Response;
  let errorMsg: string | undefined;

  try {
    response = await handler(ctx);
  } catch (err) {
    errorMsg = err instanceof Error ? err.message : String(err);
    response = NextResponse.json(
      { error: errorMsg },
      { status: 500 },
    );
  }

  const duration = performance.now() - start;

  const entry: AuditEntry = {
    method: req.method,
    path: req.nextUrl.pathname,
    table_name: ctx.table,
    operation: ctx.operation,
    record_id: ctx.recordId,
    record_ids: ctx.recordIds,
    request_body: body,
    response_status: response.status,
    row_count: ctx.rowCount,
    duration_ms: duration,
    sql_executed: ctx.sqlExecuted,
    error: errorMsg,
    ip: req.headers.get("x-forwarded-for") ?? req.headers.get("x-real-ip") ?? undefined,
    user_agent: req.headers.get("user-agent") ?? undefined,
  };

  logToConsole(db, entry);

  // Write audit asynchronously — don't block the response
  writeAudit(db, entry).catch(() => {});

  return response;
}

// ── Query audit log ────────────────────────────────────────────────

export interface AuditQueryParams {
  table?: string;
  operation?: string;
  record_id?: string;
  from?: string; // ISO date
  to?: string;   // ISO date
  page?: number;
  limit?: number;
}

export async function queryAuditLog(
  dbName: string,
  params: AuditQueryParams,
): Promise<{ rows: Record<string, unknown>[]; total: number }> {
  await ensureAuditTable(dbName);
  const sql = await getPool(dbName);

  const page = Math.max(1, params.page ?? 1);
  const limit = Math.min(200, Math.max(1, params.limit ?? 50));
  const offset = (page - 1) * limit;

  const conditions: string[] = [];
  const values: unknown[] = [];
  let idx = 0;

  if (params.table) {
    idx++;
    conditions.push(`table_name = $${idx}`);
    values.push(params.table);
  }
  if (params.operation) {
    idx++;
    conditions.push(`operation = $${idx}`);
    values.push(params.operation);
  }
  if (params.record_id) {
    idx++;
    conditions.push(`record_id = $${idx}`);
    values.push(params.record_id);
  }
  if (params.from) {
    idx++;
    conditions.push(`timestamp >= $${idx}`);
    values.push(params.from);
  }
  if (params.to) {
    idx++;
    conditions.push(`timestamp <= $${idx}`);
    values.push(params.to);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";

  idx++;
  const limitIdx = idx;
  values.push(limit);
  idx++;
  const offsetIdx = idx;
  values.push(offset);

  const rows = await sql.unsafe(
    `SELECT * FROM _stupid_audit ${where} ORDER BY timestamp DESC LIMIT $${limitIdx} OFFSET $${offsetIdx}`,
    values as never[],
  );

  const countValues = values.slice(0, -2); // Remove limit/offset
  const [countRow] = await sql.unsafe(
    `SELECT count(*)::int AS total FROM _stupid_audit ${where}`,
    countValues as never[],
  );

  return {
    rows: rows as unknown as Record<string, unknown>[],
    total: (countRow?.total as number) ?? 0,
  };
}

// ── Audit stats ────────────────────────────────────────────────────

export interface AuditStats {
  total_requests: number;
  by_table: Record<string, number>;
  by_operation: Record<string, number>;
  error_count: number;
  error_rate: number;
  avg_duration_ms: number;
  slowest: Array<{ path: string; duration_ms: number; timestamp: string }>;
}

export async function getAuditStats(dbName: string): Promise<AuditStats> {
  await ensureAuditTable(dbName);
  const sql = await getPool(dbName);

  const [totals] = await sql`
    SELECT
      count(*)::int AS total,
      count(*) FILTER (WHERE error IS NOT NULL)::int AS errors,
      COALESCE(avg(duration_ms), 0)::float AS avg_duration
    FROM _stupid_audit
  `;

  const byTable = await sql`
    SELECT table_name, count(*)::int AS cnt
    FROM _stupid_audit
    WHERE table_name IS NOT NULL
    GROUP BY table_name
    ORDER BY cnt DESC
  `;

  const byOp = await sql`
    SELECT operation, count(*)::int AS cnt
    FROM _stupid_audit
    GROUP BY operation
    ORDER BY cnt DESC
  `;

  const slowest = await sql`
    SELECT path, duration_ms::float AS duration_ms, timestamp::text AS timestamp
    FROM _stupid_audit
    ORDER BY duration_ms DESC
    LIMIT 10
  `;

  const total = (totals?.total as number) ?? 0;
  const errors = (totals?.errors as number) ?? 0;

  return {
    total_requests: total,
    by_table: Object.fromEntries(byTable.map((r) => [r.table_name, r.cnt])),
    by_operation: Object.fromEntries(byOp.map((r) => [r.operation, r.cnt])),
    error_count: errors,
    error_rate: total > 0 ? errors / total : 0,
    avg_duration_ms: (totals?.avg_duration as number) ?? 0,
    slowest: slowest.map((r) => ({
      path: r.path as string,
      duration_ms: r.duration_ms as number,
      timestamp: r.timestamp as string,
    })),
  };
}
