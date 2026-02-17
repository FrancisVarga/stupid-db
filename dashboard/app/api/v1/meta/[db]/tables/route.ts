import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { listTables, listSchemas } from "@/lib/db/introspect";

export const dynamic = "force-dynamic";

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  try {
    const sql = await getPool(db);
    const schemaParam = _req.nextUrl.searchParams.get("schema");

    // If schema=* or schema=all, return tables grouped by schema
    if (schemaParam === "*" || schemaParam === "all") {
      const schemas = await listSchemas(sql);
      const grouped: Record<string, Awaited<ReturnType<typeof listTables>>> = {};
      await Promise.all(
        schemas.map(async (s) => {
          const tables = await listTables(sql, s);
          if (tables.length > 0) grouped[s] = tables;
        }),
      );
      return NextResponse.json(grouped);
    }

    const tables = await listTables(sql, schemaParam ?? "public");
    return NextResponse.json(tables);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
