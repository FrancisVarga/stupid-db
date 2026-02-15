import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { withAudit } from "@/lib/db/logger";

export const dynamic = "force-dynamic";

interface QueryBody {
  sql: string;
  params?: unknown[];
}

export async function POST(
  req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  return withAudit(db, req, async (ctx) => {
    ctx.operation = "query";

    const body = (await req.json()) as QueryBody;

    if (!body.sql || typeof body.sql !== "string") {
      return NextResponse.json(
        { error: "sql field is required and must be a string" },
        { status: 400 },
      );
    }

    ctx.sqlExecuted = body.sql;

    // Warn on mutation queries (but still execute)
    const normalized = body.sql.trim().toLowerCase();
    const isMutation =
      normalized.startsWith("insert") ||
      normalized.startsWith("update") ||
      normalized.startsWith("delete") ||
      normalized.startsWith("drop") ||
      normalized.startsWith("alter") ||
      normalized.startsWith("truncate") ||
      normalized.startsWith("create");

    const sql = await getPool(db);
    const start = performance.now();

    const queryParams = body.params ?? [];
    const rows = await sql.unsafe(body.sql, queryParams as never[]);

    const duration_ms = performance.now() - start;

    // Extract column names from first row or from the Result object
    let columns: string[] = [];
    if (rows.length > 0) {
      columns = Object.keys(rows[0] as Record<string, unknown>);
    }

    ctx.rowCount = rows.length;

    return NextResponse.json({
      columns,
      rows: rows as unknown as Record<string, unknown>[],
      rowCount: rows.length,
      duration_ms: Math.round(duration_ms * 100) / 100,
      warning: isMutation ? "This query modifies data" : undefined,
    });
  });
}
