import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { getColumns } from "@/lib/db/introspect";

export const dynamic = "force-dynamic";

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ db: string; table: string }> },
): Promise<Response> {
  const { db, table } = await params;

  try {
    const sql = await getPool(db);
    const schema = _req.nextUrl.searchParams.get("schema") ?? "public";
    const columns = await getColumns(sql, table, schema);

    if (columns.length === 0) {
      return NextResponse.json(
        { error: `Table "${table}" not found in schema "${schema}"` },
        { status: 404 },
      );
    }

    return NextResponse.json(columns);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
