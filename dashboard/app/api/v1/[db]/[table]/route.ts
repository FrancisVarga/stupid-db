import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { getColumns, getPrimaryKeys } from "@/lib/db/introspect";
import { serializeRow } from "@/lib/db/serialize";
import {
  parseQueryParams,
  buildQuery,
  buildCountQuery,
  buildInsert,
  buildUpdate,
  buildDelete,
} from "@/lib/db/query-builder";
import { withAudit } from "@/lib/db/logger";

export const dynamic = "force-dynamic";

// ── Helpers ────────────────────────────────────────────────────────

async function getTableMeta(db: string, table: string, schema = "public") {
  const sql = await getPool(db);
  const columns = await getColumns(sql, table, schema);
  if (columns.length === 0) {
    throw new NotFoundError(`Table "${schema}"."${table}" not found`);
  }
  const pkCols = await getPrimaryKeys(sql, table, schema);
  const validColumns = new Set(columns.map((c) => c.name));
  const columnTypes = new Map(columns.map((c) => [c.name, c.udt_name]));
  return { sql, columns, pkCols, validColumns, columnTypes, schema };
}

class NotFoundError extends Error {
  constructor(msg: string) {
    super(msg);
    this.name = "NotFoundError";
  }
}

// ── GET: List with pagination, filtering, sorting ──────────────────

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string }> },
): Promise<Response> {
  const { db, table } = await params;

  const schemaParam = req.nextUrl.searchParams.get("schema") || "public";

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "list";
    ctx.table = table;

    const { sql, validColumns, columnTypes, schema } = await getTableMeta(db, table, schemaParam);
    const queryParams = parseQueryParams(req.nextUrl.searchParams);
    const q = buildQuery(queryParams, validColumns);
    const cq = buildCountQuery(queryParams, validColumns);

    const [rows, [countRow]] = await Promise.all([
      sql.unsafe(
        `SELECT ${q.selectCols} FROM "${schema}"."${table}" ${q.where} ${q.orderBy} ${q.limitOffset}`,
        q.values as never[],
      ),
      sql.unsafe(
        `SELECT count(*)::int AS total FROM "${schema}"."${table}" ${cq.where}`,
        cq.values as never[],
      ),
    ]);

    const total = (countRow?.total as number) ?? 0;
    const limit = queryParams.limit ?? 50;
    const page = queryParams.page ?? 1;

    const data = rows.map((r) => serializeRow(r as Record<string, unknown>, columnTypes));
    ctx.rowCount = data.length;

    return NextResponse.json({
      rows: data,
      total,
      page,
      limit,
    });
  });
}

// ── POST: Create record(s) ─────────────────────────────────────────

export async function POST(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string }> },
): Promise<Response> {
  const { db, table } = await params;
  const schemaParam = req.nextUrl.searchParams.get("schema") || "public";

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "create";
    ctx.table = table;

    const { sql, validColumns, columnTypes, schema } = await getTableMeta(db, table, schemaParam);
    const body = await req.json();

    const records = Array.isArray(body) ? body : [body];
    const results: Record<string, unknown>[] = [];

    for (const record of records) {
      const ins = buildInsert(table, schema, record, validColumns);
      const rows = await sql.unsafe(ins.sql, ins.values as never[]);
      if (rows[0]) {
        results.push(serializeRow(rows[0] as Record<string, unknown>, columnTypes));
      }
    }

    ctx.rowCount = results.length;

    return NextResponse.json(
      { data: results, count: results.length },
      { status: 201 },
    );
  });
}

// ── PATCH: Batch update ────────────────────────────────────────────

export async function PATCH(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string }> },
): Promise<Response> {
  const { db, table } = await params;
  const schemaParam = req.nextUrl.searchParams.get("schema") || "public";

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "batch_update";
    ctx.table = table;

    const { sql, pkCols, validColumns, columnTypes, schema } = await getTableMeta(db, table, schemaParam);

    if (pkCols.length === 0) {
      return NextResponse.json(
        { error: "Table has no primary key — batch update not supported" },
        { status: 400 },
      );
    }

    const body = await req.json();
    const { ids, updates } = body as { ids: string[]; updates: Record<string, unknown> };

    if (!Array.isArray(ids) || ids.length === 0) {
      return NextResponse.json({ error: "ids array is required" }, { status: 400 });
    }
    if (!updates || typeof updates !== "object") {
      return NextResponse.json({ error: "updates object is required" }, { status: 400 });
    }

    ctx.recordIds = ids.map(String);
    const results: Record<string, unknown>[] = [];

    for (const id of ids) {
      const pkValues = parsePkValues(String(id), pkCols);
      const upd = buildUpdate(table, schema, pkCols, pkValues, updates, validColumns);
      const rows = await sql.unsafe(upd.sql, upd.values as never[]);
      if (rows[0]) {
        results.push(serializeRow(rows[0] as Record<string, unknown>, columnTypes));
      }
    }

    ctx.rowCount = results.length;

    return NextResponse.json({
      data: results,
      count: results.length,
    });
  });
}

// ── DELETE: Batch delete ───────────────────────────────────────────

export async function DELETE(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string }> },
): Promise<Response> {
  const { db, table } = await params;
  const schemaParam = req.nextUrl.searchParams.get("schema") || "public";

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "batch_delete";
    ctx.table = table;

    const { sql, pkCols, columnTypes, schema } = await getTableMeta(db, table, schemaParam);

    if (pkCols.length === 0) {
      return NextResponse.json(
        { error: "Table has no primary key — batch delete not supported" },
        { status: 400 },
      );
    }

    const body = await req.json();
    const { ids } = body as { ids: string[] };

    if (!Array.isArray(ids) || ids.length === 0) {
      return NextResponse.json({ error: "ids array is required" }, { status: 400 });
    }

    ctx.recordIds = ids.map(String);
    const results: Record<string, unknown>[] = [];

    for (const id of ids) {
      const pkValues = parsePkValues(String(id), pkCols);
      const del = buildDelete(table, schema, pkCols, pkValues);
      const rows = await sql.unsafe(del.sql, del.values as never[]);
      if (rows[0]) {
        results.push(serializeRow(rows[0] as Record<string, unknown>, columnTypes));
      }
    }

    ctx.rowCount = results.length;

    return NextResponse.json({
      data: results,
      count: results.length,
    });
  });
}

// ── PK value parsing ───────────────────────────────────────────────

/**
 * Parse composite PK values from URL segment.
 * Single PK: "123" → ["123"]
 * Composite: "val1--val2" → ["val1", "val2"]
 */
function parsePkValues(id: string, pkCols: string[]): unknown[] {
  if (pkCols.length === 1) return [id];
  const parts = id.split("--");
  if (parts.length !== pkCols.length) {
    throw new Error(
      `Expected ${pkCols.length} PK values (separated by --), got ${parts.length}`,
    );
  }
  return parts;
}
