import { NextRequest, NextResponse } from "next/server";
import { getAuditStats } from "@/lib/db/logger";

export const dynamic = "force-dynamic";

export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  try {
    const stats = await getAuditStats(db);
    return NextResponse.json(stats);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
