/** Adapted output for the DataTableWidget. */
export interface DataTablePayload {
  columns: string[];
  rows: Record<string, unknown>[];
}

/**
 * Transform a raw API response into columns + rows for the DataTable widget.
 *
 * Handles three shapes:
 *  1. Array of objects — each object is a row, keys of the first object become columns.
 *  2. Object with an array property (e.g. `{ results: [...] }`, `{ data: [...] }`,
 *     `{ rows: [...] }`) — unwraps the first array-valued key.
 *  3. Anything else — returns empty data.
 */
export function adaptDataTableData(raw: unknown): DataTablePayload {
  const rows = extractRows(raw);
  if (rows.length === 0) return { columns: [], rows: [] };

  const columns = Object.keys(rows[0]);
  return { columns, rows };
}

function extractRows(raw: unknown): Record<string, unknown>[] {
  // Shape 1: direct array of objects
  if (Array.isArray(raw)) {
    return raw.filter(
      (item): item is Record<string, unknown> =>
        item !== null && typeof item === "object",
    );
  }

  // Shape 2: object wrapping an array
  if (raw !== null && typeof raw === "object" && !Array.isArray(raw)) {
    const obj = raw as Record<string, unknown>;
    for (const key of Object.keys(obj)) {
      if (Array.isArray(obj[key]) && (obj[key] as unknown[]).length > 0) {
        return (obj[key] as unknown[]).filter(
          (item): item is Record<string, unknown> =>
            item !== null && typeof item === "object",
        );
      }
    }
  }

  return [];
}
