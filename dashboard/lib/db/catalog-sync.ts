/**
 * Sync Database Manager connections to the catalog as ExternalSource entries.
 *
 * Introspects each PostgreSQL connection (schemas -> tables -> columns) and
 * pushes the results to the Rust backend's catalog API (`POST /catalog/externals`).
 * This makes DB Manager databases visible to the LLM system prompt alongside
 * Athena sources.
 */

import { listConnections } from "./connections";
import { getPool } from "./client";
import { listSchemas, listTables, getColumns } from "./introspect";

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

// Mirror Rust catalog types (crates/catalog/src/catalog.rs)
interface ExternalColumn {
  name: string;
  data_type: string;
}

interface ExternalTable {
  name: string;
  columns: ExternalColumn[];
}

interface ExternalDatabase {
  name: string;
  tables: ExternalTable[];
}

interface ExternalSource {
  name: string;
  kind: string;
  connection_id: string;
  databases: ExternalDatabase[];
}

interface ExternalSourceSummary {
  name: string;
  kind: string;
  connection_id: string;
  database_count: number;
}

export interface CatalogSyncResult {
  synced: number;
  removed: number;
  errors: string[];
}

/**
 * Introspect all Database Manager connections and push their schemas to the
 * catalog as `ExternalSource{kind: "postgres"}` entries.
 *
 * Also removes catalog entries for connections that no longer exist.
 * Connections that fail to connect are skipped (error recorded).
 */
export async function syncConnectionsToCatalog(): Promise<CatalogSyncResult> {
  const connections = await listConnections();
  const errors: string[] = [];
  let synced = 0;
  let removed = 0;

  // Introspect all connections in parallel
  const results = await Promise.allSettled(
    connections.map(async (conn) => {
      const sql = await getPool(conn.id);
      const schemas = await listSchemas(sql);

      const tables: ExternalTable[] = [];

      for (const schema of schemas) {
        const schemaTables = await listTables(sql, schema);

        for (const t of schemaTables) {
          const cols = await getColumns(sql, t.name, schema);
          const tableName = schema === "public" ? t.name : `${schema}.${t.name}`;

          tables.push({
            name: tableName,
            columns: cols.map((c) => ({
              name: c.name,
              data_type: c.data_type,
            })),
          });
        }
      }

      const source: ExternalSource = {
        name: conn.name,
        kind: "postgres",
        connection_id: conn.id,
        databases: [
          {
            name: conn.database,
            tables,
          },
        ],
      };

      return { conn, source };
    }),
  );

  // Push successful introspections to the catalog
  const syncedIds = new Set<string>();

  for (const result of results) {
    if (result.status === "rejected") {
      errors.push(String(result.reason));
      continue;
    }

    const { conn, source } = result.value;

    try {
      const res = await fetch(`${API_BASE}/catalog/externals`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(source),
      });

      if (!res.ok) {
        const text = await res.text().catch(() => "");
        errors.push(`Failed to sync "${conn.name}": ${res.status} ${text}`);
      } else {
        synced++;
        syncedIds.add(conn.id);
      }
    } catch (err) {
      errors.push(
        `Failed to push "${conn.name}" to catalog: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  // Clean up: remove postgres sources whose connection no longer exists
  try {
    const res = await fetch(`${API_BASE}/catalog/externals`, { cache: "no-store" });
    if (res.ok) {
      const existing: ExternalSourceSummary[] = await res.json();
      const connectionIds = new Set(connections.map((c) => c.id));

      const stale = existing.filter(
        (s) => s.kind === "postgres" && !connectionIds.has(s.connection_id),
      );

      for (const s of stale) {
        try {
          const delRes = await fetch(
            `${API_BASE}/catalog/externals/${encodeURIComponent(s.kind)}/${encodeURIComponent(s.connection_id)}`,
            { method: "DELETE" },
          );
          if (delRes.ok || delRes.status === 204) {
            removed++;
          }
        } catch {
          // Non-critical — stale entries are harmless
        }
      }
    }
  } catch {
    // Catalog may not be available yet during startup — skip cleanup
  }

  return { synced, removed, errors };
}

/**
 * Fire-and-forget catalog sync. Logs errors but does not throw.
 * Use this in CRUD handlers where sync failure shouldn't block the response.
 */
export function syncConnectionsToCatalogAsync(): void {
  syncConnectionsToCatalog().catch((err) => {
    console.error("[catalog-sync] Background sync failed:", err);
  });
}
