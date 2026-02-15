import { NextRequest, NextResponse } from "next/server";
import {
  updateQueueConnection,
  deleteQueueConnection,
  getQueueConnection,
} from "@/lib/db/queue-connections";

export const dynamic = "force-dynamic";

/** GET /api/v1/meta/queue-connections/{id} — get queue connection (credentials masked). */
export async function GET(
  _req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  const conn = await getQueueConnection(id);
  if (!conn) {
    return NextResponse.json({ error: "Queue connection not found" }, { status: 404 });
  }
  // Return safe version (mask credentials)
  const { access_key_id: _, secret_access_key: __, session_token: ___, ...safe } = conn;
  return NextResponse.json({
    ...safe,
    access_key_id: "********",
    secret_access_key: "********",
    session_token: "********",
  });
}

/** PUT /api/v1/meta/queue-connections/{id} — update a queue connection. */
export async function PUT(
  req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  try {
    const body = await req.json();
    const result = await updateQueueConnection(id, body);
    if (!result) {
      return NextResponse.json({ error: "Queue connection not found" }, { status: 404 });
    }
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** DELETE /api/v1/meta/queue-connections/{id} — remove a queue connection. */
export async function DELETE(
  _req: NextRequest,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;

  try {
    const deleted = await deleteQueueConnection(id);
    if (!deleted) {
      return NextResponse.json({ error: "Queue connection not found" }, { status: 404 });
    }
    return NextResponse.json({ ok: true });
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
