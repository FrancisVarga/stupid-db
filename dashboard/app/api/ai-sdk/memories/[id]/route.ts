import { NextRequest, NextResponse } from "next/server";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";

export const dynamic = "force-dynamic";

const migrated = ensureAiSdkTables();

type RouteContext = { params: Promise<{ id: string }> };

/** DELETE /api/ai-sdk/memories/[id] — delete a memory. */
export async function DELETE(
  _req: NextRequest,
  { params }: RouteContext,
): Promise<Response> {
  const { id } = await params;

  try {
    await migrated;

    const [deleted] = await sql`
      DELETE FROM memories
      WHERE id = ${id}
      RETURNING id
    `;

    if (!deleted) {
      return NextResponse.json({ error: "Memory not found" }, { status: 404 });
    }

    return NextResponse.json({ ok: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
