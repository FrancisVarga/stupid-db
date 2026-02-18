import { tool } from "ai";
import { z } from "zod";

const SITE_URL =
  process.env.NEXT_PUBLIC_SITE_URL || "http://localhost:3000";
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
    execute: async ({ sql }): Promise<PgQueryResult> => {
      if (!isSelectOnly(sql)) {
        return {
          error:
            "Only SELECT queries are allowed. Please rewrite as a SELECT query.",
        };
      }

      try {
        const res = await fetch(
          `${SITE_URL}/api/v1/${encodeURIComponent(connectionId)}/query`,
          {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ sql, params: [] }),
          },
        );

        if (!res.ok) {
          const text = await res.text().catch(() => "");
          return { error: `Query failed (${res.status}): ${text || res.statusText}` };
        }

        const result = (await res.json()) as {
          columns: string[];
          rows: Record<string, unknown>[];
          row_count: number;
          duration_ms: number;
        };

        const truncated = result.row_count > MAX_ROWS;
        return {
          columns: result.columns,
          rows: result.rows.slice(0, MAX_ROWS),
          rowCount: result.row_count,
          truncated,
          duration_ms: result.duration_ms,
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
