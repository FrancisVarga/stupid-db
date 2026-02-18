import { NextRequest, NextResponse } from "next/server";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";

export const dynamic = "force-dynamic";

const migrated = ensureAiSdkTables();

/** GET /api/ai-sdk/memories/search?q=... — full-text search memories. */
export async function GET(req: NextRequest): Promise<Response> {
  try {
    await migrated;

    const q = req.nextUrl.searchParams.get("q")?.trim();
    if (!q) {
      return NextResponse.json(
        { error: "q query parameter is required" },
        { status: 400 },
      );
    }

    const limit = Number(req.nextUrl.searchParams.get("limit")) || 20;

    // Convert user query to tsquery: split on whitespace, join with &
    // e.g. "database schema" → "database & schema"
    const tsquery = q
      .split(/\s+/)
      .filter(Boolean)
      .join(" & ");

    const memories = await sql`
      SELECT
        id, content, category, tags, created_at, updated_at,
        ts_rank(to_tsvector('english', content), to_tsquery('english', ${tsquery})) AS rank
      FROM memories
      WHERE to_tsvector('english', content) @@ to_tsquery('english', ${tsquery})
      ORDER BY rank DESC, updated_at DESC
      LIMIT ${limit}
    `;

    return NextResponse.json(memories);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
