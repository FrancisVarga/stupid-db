import { NextRequest, NextResponse } from "next/server";
import { listConnections } from "@/lib/db/connections";
import { getPool } from "@/lib/db/client";
import { countTables } from "@/lib/db/introspect";

export const dynamic = "force-dynamic";

/**
 * GET /api/v1/meta/databases
 *
 * Returns all registered connections enriched with table counts.
 * Each connection = one database entry in the UI.
 */
export async function GET(_req: NextRequest): Promise<Response> {
  try {
    const connections = await listConnections();

    // Enrich each connection with live stats (parallel)
    const enriched = await Promise.all(
      connections.map(async (conn) => {
        try {
          const sql = await getPool(conn.id);
          const tableCount = await countTables(sql);
          // Get database size
          const [sizeRow] = await sql`
            SELECT pg_size_pretty(pg_database_size(current_database())) AS size
          `;
          return {
            ...conn,
            table_count: tableCount,
            size: (sizeRow?.size as string) ?? "unknown",
            status: "connected" as const,
          };
        } catch (err) {
          return {
            ...conn,
            table_count: 0,
            size: "N/A",
            status: "error" as const,
            error: err instanceof Error ? err.message : String(err),
          };
        }
      }),
    );

    return NextResponse.json(enriched);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
