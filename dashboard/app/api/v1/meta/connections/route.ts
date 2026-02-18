import { NextRequest, NextResponse } from "next/server";
import {
  listConnections,
  addConnection,
  type ConnectionInput,
} from "@/lib/db/connections";
import { syncConnectionsToCatalogAsync } from "@/lib/db/catalog-sync";

export const dynamic = "force-dynamic";

/** GET /api/v1/meta/connections — list all connections (passwords masked). */
export async function GET(): Promise<Response> {
  try {
    const connections = await listConnections();
    return NextResponse.json(connections);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}

/** POST /api/v1/meta/connections — add a new connection. */
export async function POST(req: NextRequest): Promise<Response> {
  try {
    const body = (await req.json()) as ConnectionInput;

    // Validate required fields
    if (!body.name?.trim()) {
      return NextResponse.json({ error: "name is required" }, { status: 400 });
    }

    const hasConnectionString = !!body.connection_string?.trim();

    // Only require host/database when no connection string is provided
    if (!hasConnectionString) {
      if (!body.host?.trim()) {
        return NextResponse.json({ error: "host is required" }, { status: 400 });
      }
      if (!body.database?.trim()) {
        return NextResponse.json({ error: "database is required" }, { status: 400 });
      }
    }

    const conn = await addConnection({
      name: body.name.trim(),
      host: body.host?.trim() || "localhost",
      port: body.port || 5432,
      database: body.database?.trim() || "",
      username: body.username || "postgres",
      password: body.password || "",
      ssl: body.ssl ?? false,
      color: body.color || "#00f0ff",
      ...(hasConnectionString ? { connection_string: body.connection_string!.trim() } : {}),
    });

    // Sync new connection's schema to the catalog (fire-and-forget)
    syncConnectionsToCatalogAsync();

    return NextResponse.json(conn, { status: 201 });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const status = message.includes("already exists") ? 409 : 500;
    return NextResponse.json({ error: message }, { status });
  }
}
