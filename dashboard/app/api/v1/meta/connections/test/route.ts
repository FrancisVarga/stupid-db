import { NextRequest, NextResponse } from "next/server";
import { createTestPool } from "@/lib/db/client";

export const dynamic = "force-dynamic";

/** POST /api/v1/meta/connections/test — test a connection without saving it. */
export async function POST(req: NextRequest): Promise<Response> {
  let sql;
  try {
    const body = await req.json();

    if (!body.host || !body.database) {
      return NextResponse.json(
        { error: "host and database are required" },
        { status: 400 },
      );
    }

    sql = createTestPool({
      host: body.host,
      port: body.port || 5432,
      database: body.database,
      username: body.username || "postgres",
      password: body.password || "",
      ssl: body.ssl ?? false,
    });

    const start = performance.now();
    const [row] = await sql`SELECT version() AS version, current_database() AS db`;
    const duration = Math.round(performance.now() - start);

    return NextResponse.json({
      ok: true,
      version: row.version,
      database: row.db,
      duration_ms: duration,
    });
  } catch (err) {
    return NextResponse.json(
      {
        ok: false,
        error: err instanceof Error ? err.message : String(err),
      },
      { status: 400 },
    );
  } finally {
    if (sql) {
      await sql.end({ timeout: 2 }).catch(() => {});
    }
  }
}
