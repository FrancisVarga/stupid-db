import { NextRequest, NextResponse } from "next/server";
import { queryAuditLog } from "@/lib/db/logger";

export const dynamic = "force-dynamic";

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;
  const sp = req.nextUrl.searchParams;

  try {
    const result = await queryAuditLog(db, {
      table: sp.get("table") ?? undefined,
      operation: sp.get("operation") ?? undefined,
      record_id: sp.get("record_id") ?? undefined,
      from: sp.get("from") ?? undefined,
      to: sp.get("to") ?? undefined,
      page: Number(sp.get("page")) || 1,
      limit: Number(sp.get("limit")) || 50,
    });

    const page = Number(sp.get("page")) || 1;
    const limit = Number(sp.get("limit")) || 50;

    return NextResponse.json({
      rows: result.rows,
      total: result.total,
      page,
      limit,
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
