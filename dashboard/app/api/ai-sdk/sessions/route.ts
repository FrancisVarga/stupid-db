import { NextRequest, NextResponse } from "next/server";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";

export const dynamic = "force-dynamic";

// Run migration lazily on first request
const migrated = ensureAiSdkTables();

/** GET /api/ai-sdk/sessions — list all sessions, most recent first. */
export async function GET(): Promise<Response> {
  try {
    await migrated;
    const sessions = await sql`
      SELECT id, title, provider, model, created_at, updated_at
      FROM chat_sessions
      ORDER BY updated_at DESC
    `;
    return NextResponse.json(sessions);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** POST /api/ai-sdk/sessions — create a new session. */
export async function POST(req: NextRequest): Promise<Response> {
  try {
    await migrated;
    const body = (await req.json()) as {
      title?: string;
      provider?: string;
      model?: string;
    };

    const [session] = await sql`
      INSERT INTO chat_sessions (title, provider, model)
      VALUES (
        ${body.title ?? "New Chat"},
        ${body.provider ?? "anthropic"},
        ${body.model ?? "claude-sonnet-4-6"}
      )
      RETURNING id, title, provider, model, created_at, updated_at
    `;

    return NextResponse.json(session, { status: 201 });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
