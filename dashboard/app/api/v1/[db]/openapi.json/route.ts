import { NextRequest, NextResponse } from "next/server";
import { getPool } from "@/lib/db/client";
import { generateOpenAPISpec } from "@/lib/db/openapi-gen";

export const dynamic = "force-dynamic";

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  try {
    const sql = await getPool(db);
    const proto = req.headers.get("x-forwarded-proto") ?? "http";
    const host = req.headers.get("host") ?? "localhost:39300";
    const baseUrl = `${proto}://${host}`;

    const spec = await generateOpenAPISpec(sql, db, baseUrl);

    return NextResponse.json(spec, {
      headers: {
        "Cache-Control": "public, max-age=60",
      },
    });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
