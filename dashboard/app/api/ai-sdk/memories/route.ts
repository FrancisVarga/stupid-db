import { NextRequest, NextResponse } from "next/server";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";

export const dynamic = "force-dynamic";

const migrated = ensureAiSdkTables();

/** GET /api/ai-sdk/memories — list memories with optional tag/category filter. */
export async function GET(req: NextRequest): Promise<Response> {
  try {
    await migrated;

    const tag = req.nextUrl.searchParams.get("tag");
    const category = req.nextUrl.searchParams.get("category");
    const limit = Number(req.nextUrl.searchParams.get("limit")) || 50;
    const offset = Number(req.nextUrl.searchParams.get("offset")) || 0;

    let memories;

    if (tag && category) {
      memories = await sql`
        SELECT id, content, category, tags, created_at, updated_at
        FROM memories
        WHERE tags @> ARRAY[${tag}]::text[]
          AND category = ${category}
        ORDER BY updated_at DESC
        LIMIT ${limit} OFFSET ${offset}
      `;
    } else if (tag) {
      memories = await sql`
        SELECT id, content, category, tags, created_at, updated_at
        FROM memories
        WHERE tags @> ARRAY[${tag}]::text[]
        ORDER BY updated_at DESC
        LIMIT ${limit} OFFSET ${offset}
      `;
    } else if (category) {
      memories = await sql`
        SELECT id, content, category, tags, created_at, updated_at
        FROM memories
        WHERE category = ${category}
        ORDER BY updated_at DESC
        LIMIT ${limit} OFFSET ${offset}
      `;
    } else {
      memories = await sql`
        SELECT id, content, category, tags, created_at, updated_at
        FROM memories
        ORDER BY updated_at DESC
        LIMIT ${limit} OFFSET ${offset}
      `;
    }

    return NextResponse.json(memories);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** POST /api/ai-sdk/memories — save a new memory. */
export async function POST(req: NextRequest): Promise<Response> {
  try {
    await migrated;
    const body = (await req.json()) as {
      content: string;
      category?: string;
      tags?: string[];
    };

    if (!body.content?.trim()) {
      return NextResponse.json(
        { error: "content is required" },
        { status: 400 },
      );
    }

    const [memory] = await sql`
      INSERT INTO memories (content, category, tags)
      VALUES (
        ${body.content},
        ${body.category ?? null},
        ${body.tags ?? []}
      )
      RETURNING id, content, category, tags, created_at, updated_at
    `;

    return NextResponse.json(memory, { status: 201 });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
