// ── Athena schema → LLM prompt formatter ─────────────────────────────
//
// Converts an AthenaSchema object into a compact text representation
// suitable for injection into an LLM system prompt.

import type { AthenaSchema } from "@/lib/db/athena-connections";

/** Max characters for schema text before truncation. */
const MAX_SCHEMA_CHARS = 50_000;

/**
 * Format an Athena schema into a compact, LLM-friendly text block.
 *
 * Output format:
 *   Database: my_db
 *     Table: users (id bigint, name string, email string, created_at timestamp)
 *     Table: orders (order_id bigint, user_id bigint, total decimal)
 */
export function formatSchemaForPrompt(schema: AthenaSchema | null): string {
  if (!schema || schema.databases.length === 0) {
    return "No schema available. Ask the user to refresh the schema first.";
  }

  const lines: string[] = [];
  let totalTables = 0;
  let totalColumns = 0;

  for (const db of schema.databases) {
    lines.push(`Database: ${db.name}`);
    for (const table of db.tables) {
      totalTables++;
      const cols = table.columns.map((c) => {
        totalColumns++;
        const comment = c.comment ? ` -- ${c.comment}` : "";
        return `${c.name} ${c.data_type}${comment}`;
      });
      lines.push(`  Table: ${table.name} (${cols.join(", ")})`);
    }
  }

  let text = lines.join("\n");

  // Truncate if too large for context window
  if (text.length > MAX_SCHEMA_CHARS) {
    text = text.slice(0, MAX_SCHEMA_CHARS);
    // Cut at last complete line
    const lastNewline = text.lastIndexOf("\n");
    if (lastNewline > 0) text = text.slice(0, lastNewline);
    text += `\n\n... schema truncated (${totalTables} tables, ${totalColumns} columns total)`;
  }

  return text;
}

/**
 * Count databases and tables in a schema for summary display.
 */
export function schemaStats(schema: AthenaSchema | null): {
  databases: number;
  tables: number;
  columns: number;
} {
  if (!schema) return { databases: 0, tables: 0, columns: 0 };

  let tables = 0;
  let columns = 0;
  for (const db of schema.databases) {
    for (const table of db.tables) {
      tables++;
      columns += table.columns.length;
    }
  }
  return { databases: schema.databases.length, tables, columns };
}
