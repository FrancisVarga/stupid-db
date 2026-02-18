import { tool } from "ai";
import { z } from "zod";
import { getPool } from "@/lib/db/client";

const MAX_ROWS = 100;

/**
 * Server-side SQL validation — only SELECT statements are permitted.
 * Strips SQL comments and leading whitespace before checking.
 */
function isSelectOnly(sql: string): boolean {
  const cleaned = sql
    .replace(/--[^\n]*/g, "")
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .trim();
  return /^select\b/i.test(cleaned);
}

interface PgQueryResult {
  columns?: string[];
  rows?: Record<string, unknown>[];
  rowCount?: number;
  truncated?: boolean;
  duration_ms?: number;
  error?: string;
}

/**
 * Factory that creates a pg_query tool bound to a specific connection ID.
 * The AI only supplies the `sql` parameter; the connection is fixed at the
 * route level so the model cannot target arbitrary databases.
 *
 * Executes queries directly via the connection pool instead of making an
 * HTTP self-call, avoiding port mismatch issues in dev.
 */
export function createPgQueryTool(connectionId: string) {
  return tool({
    description:
      "Execute a read-only SQL query against the connected PostgreSQL database. " +
      "Use this to answer questions about data by writing SELECT queries. " +
      "Only SELECT queries are allowed — mutations will be rejected.",
    inputSchema: z.object({
      sql: z.string().describe("The SQL SELECT query to execute"),
    }),
    execute: async ({ sql: query }): Promise<PgQueryResult> => {
      if (!isSelectOnly(query)) {
        return {
          error:
            "Only SELECT queries are allowed. Please rewrite as a SELECT query.",
        };
      }

      try {
        const pool = await getPool(connectionId);
        const start = performance.now();
        const rows = await pool.unsafe(query);
        const duration_ms = performance.now() - start;

        let columns: string[] = [];
        if (rows.length > 0) {
          columns = Object.keys(rows[0] as Record<string, unknown>);
        }

        const truncated = rows.length > MAX_ROWS;
        return {
          columns,
          rows: (rows as unknown as Record<string, unknown>[]).slice(0, MAX_ROWS),
          rowCount: rows.length,
          truncated,
          duration_ms: Math.round(duration_ms * 100) / 100,
        };
      } catch (error) {
        return {
          error:
            error instanceof Error
              ? error.message
              : "Unknown error executing query",
        };
      }
    },
  });
}
