import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { listTables } from "@/lib/db/introspect";

export const dynamic = "force-dynamic";

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  try {
    const sql = await getPool(db);
    const schema = _req.nextUrl.searchParams.get("schema") ?? "public";
    const tables = await listTables(sql, schema);
    return NextResponse.json(tables);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
