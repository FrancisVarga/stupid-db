// ── Postgres schema → LLM prompt formatter ──────────────────────────
//
// Fetches table + column metadata for a database connection and formats
// it as DDL-like text suitable for injection into an LLM system prompt.

import {
  fetchTables,
  fetchTableSchema,
  type Table,
  type Column,
} from "@/lib/api-db";

/** Max characters for schema text before truncation. */
const MAX_SCHEMA_CHARS = 50_000;

/**
 * Format a row count estimate as a human-readable string.
 * e.g. 50000 → "50.0K", 1200000 → "1.2M", 800 → "800"
 */
function formatRowCount(rows: number): string {
  if (rows >= 1_000_000) return `${(rows / 1_000_000).toFixed(1)}M`;
  if (rows >= 1_000) return `${(rows / 1_000).toFixed(1)}K`;
  return String(rows);
}

/**
 * Format a single column as a DDL-like annotation string.
 *
 * Example outputs:
 *   id integer [PK]
 *   email varchar [NOT NULL, UNIQUE]
 *   company_id integer [FK → companies.id]
 */
function formatColumn(col: Column): string {
  const annotations: string[] = [];

  if (col.is_pk) annotations.push("PK");
  if (!col.nullable && !col.is_pk) annotations.push("NOT NULL");
  if (col.is_unique && !col.is_pk) annotations.push("UNIQUE");
  if (col.is_indexed && !col.is_pk && !col.is_unique) annotations.push("INDEXED");
  if (col.fk_target) annotations.push(`FK \u2192 ${col.fk_target}`);

  const suffix = annotations.length > 0 ? ` [${annotations.join(", ")}]` : "";
  return `--   ${col.name} ${col.type}${suffix}`;
}

/**
 * Format a table header + its columns into DDL-like comment lines.
 */
function formatTable(table: Table, columns: Column[]): string {
  const rowLabel = formatRowCount(table.estimated_rows);
  const typeLabel = table.type === "view" ? "View" : "Table";
  const header = `-- ${typeLabel}: ${table.schema}.${table.name} (~${rowLabel} rows, ${table.size})`;
  const colLines = columns.map(formatColumn);
  return [header, ...colLines].join("\n");
}

/**
 * Fetch the full schema for a database and format it as an LLM-friendly
 * DDL-like text block.
 *
 * 1. Fetches all tables via `fetchTables(dbId)`
 * 2. Batches all `fetchTableSchema()` calls with `Promise.all`
 * 3. Formats each table + columns as annotated DDL comments
 * 4. Truncates if the result exceeds `MAX_SCHEMA_CHARS`
 *
 * @param dbId - The database connection ID
 * @returns Formatted schema string ready for system prompt injection
 */
export async function buildSchemaContext(dbId: string): Promise<string> {
  const tables = await fetchTables(dbId);

  if (tables.length === 0) {
    return `Database Schema for "${dbId}":\n\nNo tables found.`;
  }

  // Batch all column fetches in parallel
  const columnResults = await Promise.all(
    tables.map((t) => fetchTableSchema(dbId, t.name, t.schema)),
  );

  const tableBlocks: string[] = [];
  let totalColumns = 0;

  for (let i = 0; i < tables.length; i++) {
    const columns = columnResults[i];
    totalColumns += columns.length;
    tableBlocks.push(formatTable(tables[i], columns));
  }

  let text = `Database Schema for "${dbId}":\n\n${tableBlocks.join("\n\n")}`;

  // Truncate if too large for context window
  if (text.length > MAX_SCHEMA_CHARS) {
    text = text.slice(0, MAX_SCHEMA_CHARS);
    // Cut at last complete line
    const lastNewline = text.lastIndexOf("\n");
    if (lastNewline > 0) text = text.slice(0, lastNewline);
    text += `\n\n... schema truncated (${tables.length} tables, ${totalColumns} columns total)`;
  }

  return text;
}
