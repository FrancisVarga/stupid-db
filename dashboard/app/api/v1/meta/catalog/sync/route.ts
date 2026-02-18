import { NextResponse } from "next/server";
import { syncConnectionsToCatalog } from "@/lib/db/catalog-sync";

export const dynamic = "force-dynamic";

/**
 * POST /api/v1/meta/catalog/sync
 *
 * Introspect all Database Manager connections and push their schemas
 * to the catalog as ExternalSource{kind: "postgres"} entries.
 */
export async function POST(): Promise<Response> {
  try {
    const result = await syncConnectionsToCatalog();
    return NextResponse.json(result);
  } catch (err) {
    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 500 },
    );
  }
}
