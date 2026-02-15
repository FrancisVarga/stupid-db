import { NextRequest, NextResponse } from "next/server";
import {
  listQueueConnections,
  addQueueConnection,
} from "@/lib/db/queue-connections";

export const dynamic = "force-dynamic";

/** GET /api/v1/meta/queue-connections — list all queue connections (credentials masked). */
export async function GET(): Promise<Response> {
  try {
    const connections = await listQueueConnections();
    return NextResponse.json(connections);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** POST /api/v1/meta/queue-connections — add a new queue connection. */
export async function POST(req: NextRequest): Promise<Response> {
  try {
    const body = await req.json();

    if (!body.name?.trim()) {
      return NextResponse.json({ error: "name is required" }, { status: 400 });
    }
    if (!body.queue_url?.trim()) {
      return NextResponse.json(
        { error: "queue_url is required" },
        { status: 400 },
      );
    }

    const conn = await addQueueConnection({
      name: body.name.trim(),
      queue_url: body.queue_url.trim(),
      dlq_url: body.dlq_url?.trim() || null,
      provider: body.provider || "sqs",
      enabled: body.enabled ?? true,
      region: body.region || "ap-southeast-1",
      access_key_id: body.access_key_id || "",
      secret_access_key: body.secret_access_key || "",
      session_token: body.session_token || "",
      endpoint_url: body.endpoint_url?.trim() || null,
      poll_interval_ms: body.poll_interval_ms ?? 1000,
      max_batch_size: body.max_batch_size ?? 10,
      visibility_timeout_secs: body.visibility_timeout_secs ?? 30,
      micro_batch_size: body.micro_batch_size ?? 100,
      micro_batch_timeout_ms: body.micro_batch_timeout_ms ?? 1000,
      color: body.color || "#ff8a00",
    });

    return NextResponse.json(conn, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const status = message.includes("already exists") ? 409 : 500;
    return NextResponse.json({ error: message }, { status });
  }
}
