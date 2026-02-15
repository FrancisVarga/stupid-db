import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

const BACKEND =
  process.env.BACKEND_URL || process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

/**
 * GET /api/v1/catalog
 *
 * Thin proxy to Rust backend GET /catalog.
 * Useful when the browser cannot reach the Rust backend directly.
 */
export async function GET(): Promise<Response> {
  try {
    const res = await fetch(`${BACKEND}/catalog`, { cache: "no-store" });

    if (!res.ok) {
      const text = await res.text().catch(() => "");
      return NextResponse.json(
        { error: text || "Catalog not ready" },
        { status: res.status },
      );
    }

    const data = await res.json();
    return NextResponse.json(data);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 503 },
    );
  }
}
