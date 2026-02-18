import { NextRequest, NextResponse } from "next/server";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";

export const dynamic = "force-dynamic";

const migrated = ensureAiSdkTables();

type RouteContext = { params: Promise<{ id: string }> };

/** GET /api/ai-sdk/sessions/[id] — get session with messages. */
export async function GET(
  req: NextRequest,
  { params }: RouteContext,
): Promise<Response> {
  const { id } = await params;

  try {
    await migrated;

    const [session] = await sql`
      SELECT id, title, provider, model, created_at, updated_at
      FROM chat_sessions
      WHERE id = ${id}
    `;

    if (!session) {
      return NextResponse.json({ error: "Session not found" }, { status: 404 });
    }

    // Pagination: last N messages (default 50)
    const limit = Number(req.nextUrl.searchParams.get("limit")) || 50;
    const offset = Number(req.nextUrl.searchParams.get("offset")) || 0;

    const messages = await sql`
      SELECT id, session_id, role, content, metadata, created_at
      FROM chat_messages
      WHERE session_id = ${id}
      ORDER BY created_at ASC
      LIMIT ${limit} OFFSET ${offset}
    `;

    return NextResponse.json({ ...session, messages });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** PATCH /api/ai-sdk/sessions/[id] — update title or model. */
export async function PATCH(
  req: NextRequest,
  { params }: RouteContext,
): Promise<Response> {
  const { id } = await params;

  try {
    await migrated;
    const body = (await req.json()) as {
      title?: string;
      provider?: string;
      model?: string;
    };

    // Build SET clause dynamically for provided fields
    const updates: Record<string, string> = {};
    if (body.title !== undefined) updates.title = body.title;
    if (body.provider !== undefined) updates.provider = body.provider;
    if (body.model !== undefined) updates.model = body.model;

    if (Object.keys(updates).length === 0) {
      return NextResponse.json(
        { error: "No fields to update" },
        { status: 400 },
      );
    }

    const [session] = await sql`
      UPDATE chat_sessions SET
        ${sql(updates)},
        updated_at = now()
      WHERE id = ${id}
      RETURNING id, title, provider, model, created_at, updated_at
    `;

    if (!session) {
      return NextResponse.json({ error: "Session not found" }, { status: 404 });
    }

    return NextResponse.json(session);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** DELETE /api/ai-sdk/sessions/[id] — delete session (cascades messages). */
export async function DELETE(
  _req: NextRequest,
  { params }: RouteContext,
): Promise<Response> {
  const { id } = await params;

  try {
    await migrated;

    const [deleted] = await sql`
      DELETE FROM chat_sessions
      WHERE id = ${id}
      RETURNING id
    `;

    if (!deleted) {
      return NextResponse.json({ error: "Session not found" }, { status: 404 });
    }

    return NextResponse.json({ ok: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
