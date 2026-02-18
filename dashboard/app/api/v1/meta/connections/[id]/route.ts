import { NextRequest, NextResponse } from "next/server";
import {
  updateConnection,
  deleteConnection,
  getConnection,
} from "@/lib/db/connections";
import { invalidatePool } from "@/lib/db/client";
import { syncConnectionsToCatalogAsync } from "@/lib/db/catalog-sync";

export const dynamic = "force-dynamic";

/** PUT /api/v1/meta/connections/{id} — update a connection. */
export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  try {
    const body = await req.json();
    const result = await updateConnection(id, body);
    if (!result) {
      return NextResponse.json({ error: "Connection not found" }, { status: 404 });
    }
    // Invalidate cached pool so next request uses new config
    invalidatePool(id);
    // Sync updated schema to catalog (fire-and-forget)
    syncConnectionsToCatalogAsync();
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** DELETE /api/v1/meta/connections/{id} — remove a connection. */
export async function DELETE(
  _req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  try {
    const deleted = await deleteConnection(id);
    if (!deleted) {
      return NextResponse.json({ error: "Connection not found" }, { status: 404 });
    }
    invalidatePool(id);
    // Sync catalog to remove deleted connection's schema (fire-and-forget)
    syncConnectionsToCatalogAsync();
    return NextResponse.json({ ok: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** GET /api/v1/meta/connections/{id} — get connection details (password masked). */
export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  const conn = await getConnection(id);
  if (!conn) {
    return NextResponse.json({ error: "Connection not found" }, { status: 404 });
  }
  // Return safe version (mask password)
  const { password: _, ...safe } = conn;
  return NextResponse.json({ ...safe, password: "********" });
}
