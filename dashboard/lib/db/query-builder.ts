// ── Types ──────────────────────────────────────────────────────────

export interface QueryParams {
  page?: number;
  limit?: number;
  sort?: string;
  order?: "asc" | "desc";
  filter?: Record<string, unknown>;
  search?: string;
  search_cols?: string[];
  select?: string[];
}

interface BuiltQuery {
  where: string;
  orderBy: string;
  limitOffset: string;
  values: unknown[];
  selectCols: string;
}

// Valid column name pattern — prevents SQL injection in identifiers
const SAFE_IDENT = /^[a-zA-Z_][a-zA-Z0-9_]*$/;

// ── Filter operators ───────────────────────────────────────────────

const OPERATORS: Record<string, string> = {
  __gte: ">=",
  __lte: "<=",
  __gt: ">",
  __lt: "<",
  __ne: "!=",
  __like: "LIKE",
  __ilike: "ILIKE",
  __in: "IN",
  __is_null: "IS NULL",
};

function assertSafeIdent(name: string): void {
  if (!SAFE_IDENT.test(name)) {
    throw new Error(`Invalid column name: ${name}`);
  }
}

// ── Parser ─────────────────────────────────────────────────────────

/**
 * Parse query parameters from a URL search params object.
 */
export function parseQueryParams(searchParams: URLSearchParams): QueryParams {
  const page = Math.max(1, Number(searchParams.get("page")) || 1);
  const limit = Math.min(1000, Math.max(1, Number(searchParams.get("limit")) || 50));
  const sort = searchParams.get("sort") ?? undefined;
  const orderParam = searchParams.get("order")?.toLowerCase();
  const order = orderParam === "asc" ? "asc" : "desc";

  let filter: Record<string, unknown> | undefined;
  const filterStr = searchParams.get("filter");
  if (filterStr) {
    try {
      filter = JSON.parse(filterStr);
    } catch {
      throw new Error("Invalid filter JSON");
    }
  }

  const search = searchParams.get("search") ?? undefined;
  const searchColsStr = searchParams.get("search_cols");
  const search_cols = searchColsStr ? searchColsStr.split(",").map((s) => s.trim()) : undefined;

  const selectStr = searchParams.get("select");
  const select = selectStr ? selectStr.split(",").map((s) => s.trim()) : undefined;

  return { page, limit, sort, order, filter, search, search_cols, select };
}

// ── Builder ────────────────────────────────────────────────────────

/**
 * Build a parameterized SELECT query from parsed query params.
 * Returns SQL fragments and parameter values — caller assembles with table name.
 *
 * Usage:
 *   const q = buildQuery(params, validColumns);
 *   const rows = await sql.unsafe(
 *     `SELECT ${q.selectCols} FROM "my_table" ${q.where} ${q.orderBy} ${q.limitOffset}`,
 *     q.values
 *   );
 */
export function buildQuery(
  params: QueryParams,
  validColumns: Set<string>,
): BuiltQuery {
  const values: unknown[] = [];
  let paramIdx = 0;
  const conditions: string[] = [];

  // ── Select columns ──
  let selectCols = "*";
  if (params.select && params.select.length > 0) {
    for (const col of params.select) {
      assertSafeIdent(col);
      if (!validColumns.has(col)) {
        throw new Error(`Unknown column in select: ${col}`);
      }
    }
    selectCols = params.select.map((c) => `"${c}"`).join(", ");
  }

  // ── Filters ──
  if (params.filter) {
    for (const [rawKey, rawValue] of Object.entries(params.filter)) {
      // Check for operator suffix
      let col = rawKey;
      let op = "=";
      let isNull = false;

      for (const [suffix, sqlOp] of Object.entries(OPERATORS)) {
        if (rawKey.endsWith(suffix)) {
          col = rawKey.slice(0, -suffix.length);
          op = sqlOp;
          if (suffix === "__is_null") isNull = true;
          break;
        }
      }

      assertSafeIdent(col);
      if (!validColumns.has(col)) {
        throw new Error(`Unknown filter column: ${col}`);
      }

      if (isNull) {
        const isTrue = rawValue === true || rawValue === "true" || rawValue === 1;
        conditions.push(`"${col}" ${isTrue ? "IS NULL" : "IS NOT NULL"}`);
      } else if (op === "IN") {
        // __in expects an array
        if (!Array.isArray(rawValue)) {
          throw new Error(`Filter __in for "${col}" must be an array`);
        }
        const placeholders = rawValue.map(() => {
          paramIdx++;
          return `$${paramIdx}`;
        });
        values.push(...rawValue);
        conditions.push(`"${col}" IN (${placeholders.join(", ")})`);
      } else {
        paramIdx++;
        values.push(rawValue);
        conditions.push(`"${col}" ${op} $${paramIdx}`);
      }
    }
  }

  // ── Search (ILIKE across multiple columns) ──
  if (params.search && params.search_cols && params.search_cols.length > 0) {
    paramIdx++;
    values.push(`%${params.search}%`);
    const searchConds = params.search_cols.map((col) => {
      assertSafeIdent(col);
      if (!validColumns.has(col)) {
        throw new Error(`Unknown search column: ${col}`);
      }
      return `"${col}"::text ILIKE $${paramIdx}`;
    });
    conditions.push(`(${searchConds.join(" OR ")})`);
  }

  const where = conditions.length > 0 ? `WHERE ${conditions.join(" AND ")}` : "";

  // ── Order by ──
  let orderBy = "";
  if (params.sort) {
    assertSafeIdent(params.sort);
    if (!validColumns.has(params.sort)) {
      throw new Error(`Unknown sort column: ${params.sort}`);
    }
    orderBy = `ORDER BY "${params.sort}" ${params.order === "asc" ? "ASC" : "DESC"}`;
  }

  // ── Limit / Offset ──
  const limit = params.limit ?? 50;
  const offset = ((params.page ?? 1) - 1) * limit;
  paramIdx++;
  values.push(limit);
  const limitParam = paramIdx;
  paramIdx++;
  values.push(offset);
  const offsetParam = paramIdx;
  const limitOffset = `LIMIT $${limitParam} OFFSET $${offsetParam}`;

  return { where, orderBy, limitOffset, values, selectCols };
}

// ── Count query builder ────────────────────────────────────────────

/**
 * Build a parameterized COUNT query using the same filters.
 * Returns {countWhere, countValues} with the WHERE clause only (no ORDER/LIMIT).
 */
export function buildCountQuery(
  params: QueryParams,
  validColumns: Set<string>,
): { where: string; values: unknown[] } {
  // Reuse buildQuery but only take where + filter values (exclude limit/offset values)
  const full = buildQuery(params, validColumns);
  // The last 2 values are always limit and offset
  const filterValues = full.values.slice(0, -2);
  return { where: full.where, values: filterValues };
}

// ── INSERT builder ─────────────────────────────────────────────────

export interface InsertResult {
  sql: string;
  values: unknown[];
}

/**
 * Build a parameterized INSERT statement.
 */
export function buildInsert(
  table: string,
  schema: string,
  data: Record<string, unknown>,
  validColumns: Set<string>,
): InsertResult {
  assertSafeIdent(table);
  assertSafeIdent(schema);

  const cols: string[] = [];
  const placeholders: string[] = [];
  const values: unknown[] = [];
  let idx = 0;

  for (const [key, value] of Object.entries(data)) {
    assertSafeIdent(key);
    if (!validColumns.has(key)) {
      throw new Error(`Unknown column: ${key}`);
    }
    idx++;
    cols.push(`"${key}"`);
    placeholders.push(`$${idx}`);
    values.push(value);
  }

  if (cols.length === 0) {
    throw new Error("No columns to insert");
  }

  const sql = `INSERT INTO "${schema}"."${table}" (${cols.join(", ")}) VALUES (${placeholders.join(", ")}) RETURNING *`;
  return { sql, values };
}

// ── UPDATE builder ─────────────────────────────────────────────────

export function buildUpdate(
  table: string,
  schema: string,
  pkColumns: string[],
  pkValues: unknown[],
  data: Record<string, unknown>,
  validColumns: Set<string>,
): InsertResult {
  assertSafeIdent(table);
  assertSafeIdent(schema);

  const setClauses: string[] = [];
  const values: unknown[] = [];
  let idx = 0;

  for (const [key, value] of Object.entries(data)) {
    assertSafeIdent(key);
    if (!validColumns.has(key)) {
      throw new Error(`Unknown column: ${key}`);
    }
    if (pkColumns.includes(key)) continue; // Skip PK columns in SET
    idx++;
    setClauses.push(`"${key}" = $${idx}`);
    values.push(value);
  }

  if (setClauses.length === 0) {
    throw new Error("No columns to update");
  }

  const whereParts = pkColumns.map((col) => {
    assertSafeIdent(col);
    idx++;
    values.push(pkValues[pkColumns.indexOf(col)]);
    return `"${col}" = $${idx}`;
  });

  const sql = `UPDATE "${schema}"."${table}" SET ${setClauses.join(", ")} WHERE ${whereParts.join(" AND ")} RETURNING *`;
  return { sql, values };
}

// ── DELETE builder ─────────────────────────────────────────────────

export function buildDelete(
  table: string,
  schema: string,
  pkColumns: string[],
  pkValues: unknown[],
): InsertResult {
  assertSafeIdent(table);
  assertSafeIdent(schema);

  const values: unknown[] = [];
  let idx = 0;

  const whereParts = pkColumns.map((col, i) => {
    assertSafeIdent(col);
    idx++;
    values.push(pkValues[i]);
    return `"${col}" = $${idx}`;
  });

  const sql = `DELETE FROM "${schema}"."${table}" WHERE ${whereParts.join(" AND ")} RETURNING *`;
  return { sql, values };
}
