import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { getColumns, getPrimaryKeys } from "@/lib/db/introspect";
import { serializeRow } from "@/lib/db/serialize";
import { buildUpdate, buildDelete } from "@/lib/db/query-builder";
import { withAudit } from "@/lib/db/logger";

export const dynamic = "force-dynamic";

// ── Helpers ────────────────────────────────────────────────────────

async function getTableMeta(db: string, table: string) {
  const sql = await getPool(db);
  const columns = await getColumns(sql, table);
  if (columns.length === 0) {
    throw new Error(`Table "${table}" not found`);
  }
  const pkCols = await getPrimaryKeys(sql, table);
  if (pkCols.length === 0) {
    throw new Error(`Table "${table}" has no primary key`);
  }
  const validColumns = new Set(columns.map((c) => c.name));
  const columnTypes = new Map(columns.map((c) => [c.name, c.udt_name]));
  return { sql, columns, pkCols, validColumns, columnTypes };
}

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

function buildPkWhere(pkCols: string[], pkValues: unknown[]): { clause: string; values: unknown[] } {
  const parts = pkCols.map((col, i) => `"${col}" = $${i + 1}`);
  return { clause: `WHERE ${parts.join(" AND ")}`, values: pkValues };
}

// ── GET: Single record by PK ───────────────────────────────────────

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string; id: string }> },
): Promise<Response> {
  const { db, table, id } = await params;

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "get";
    ctx.table = table;
    ctx.recordId = id;

    const { sql, pkCols, columnTypes } = await getTableMeta(db, table);
    const pkValues = parsePkValues(id, pkCols);
    const pk = buildPkWhere(pkCols, pkValues);

    const rows = await sql.unsafe(
      `SELECT * FROM "public"."${table}" ${pk.clause}`,
      pk.values as never[],
    );

    if (rows.length === 0) {
      return NextResponse.json({ error: "Record not found" }, { status: 404 });
    }

    ctx.rowCount = 1;
    const data = serializeRow(rows[0] as Record<string, unknown>, columnTypes);
    return NextResponse.json(data);
  });
}

// ── PUT: Update single record ──────────────────────────────────────

export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string; id: string }> },
): Promise<Response> {
  const { db, table, id } = await params;

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "update";
    ctx.table = table;
    ctx.recordId = id;

    const { sql, pkCols, validColumns, columnTypes } = await getTableMeta(db, table);
    const pkValues = parsePkValues(id, pkCols);
    const body = await req.json();

    const upd = buildUpdate(table, "public", pkCols, pkValues, body, validColumns);
    const rows = await sql.unsafe(upd.sql, upd.values as never[]);

    if (rows.length === 0) {
      return NextResponse.json({ error: "Record not found" }, { status: 404 });
    }

    ctx.rowCount = 1;
    const data = serializeRow(rows[0] as Record<string, unknown>, columnTypes);
    return NextResponse.json(data);
  });
}

// ── DELETE: Delete single record ───────────────────────────────────

export async function DELETE(
  req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string; id: string }> },
): Promise<Response> {
  const { db, table, id } = await params;

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "delete";
    ctx.table = table;
    ctx.recordId = id;

    const { sql, pkCols, columnTypes } = await getTableMeta(db, table);
    const pkValues = parsePkValues(id, pkCols);

    const del = buildDelete(table, "public", pkCols, pkValues);
    const rows = await sql.unsafe(del.sql, del.values as never[]);

    if (rows.length === 0) {
      return NextResponse.json({ error: "Record not found" }, { status: 404 });
    }

    ctx.rowCount = 1;
    const data = serializeRow(rows[0] as Record<string, unknown>, columnTypes);
    return NextResponse.json(data);
  });
}
