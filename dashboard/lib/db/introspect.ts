import type postgres from "postgres";

// ── Types ──────────────────────────────────────────────────────────

export interface DatabaseInfo {
  name: string;
  size: string;
  size_bytes: number;
  table_count: number;
}

export interface TableInfo {
  schema: string;
  name: string;
  type: "table" | "view";
  estimated_rows: number;
  size: string;
  size_bytes: number;
  has_pk: boolean;
}

export interface ColumnInfo {
  name: string;
  data_type: string;
  udt_name: string;
  ordinal_position: number;
  nullable: boolean;
  column_default: string | null;
  is_pk: boolean;
  is_unique: boolean;
  is_indexed: boolean;
  fk_target: string | null; // "schema.table.column" or null
  max_length: number | null;
}

// ── Queries ────────────────────────────────────────────────────────

/**
 * List all user databases (excludes templates and system DBs).
 */
export async function listDatabases(sql: postgres.Sql): Promise<DatabaseInfo[]> {
  const rows = await sql`
    SELECT
      d.datname                              AS name,
      pg_database_size(d.datname)            AS size_bytes,
      pg_size_pretty(pg_database_size(d.datname)) AS size
    FROM pg_database d
    WHERE d.datistemplate = false
      AND d.datname NOT IN ('template0', 'template1')
    ORDER BY d.datname
  `;

  // We need a separate query per DB for table counts, but we can approximate
  // by returning 0 here and letting the caller enrich if needed.
  // For efficiency, we'll do a single query against each DB pool lazily.
  return rows.map((r) => ({
    name: r.name as string,
    size: r.size as string,
    size_bytes: Number(r.size_bytes),
    table_count: 0, // enriched by caller or separate endpoint
  }));
}

/**
 * Enrich a database entry with its table count.
 * Must be called with a pool connected to the target database.
 */
export async function countTables(sql: postgres.Sql): Promise<number> {
  const [row] = await sql`
    SELECT count(*)::int AS cnt
    FROM information_schema.tables
    WHERE table_schema NOT IN ('pg_catalog', 'information_schema')
      AND table_type IN ('BASE TABLE', 'VIEW')
  `;
  return (row?.cnt as number) ?? 0;
}

/**
 * List tables and views in a database (default schema: public).
 */
export async function listTables(
  sql: postgres.Sql,
  schema = "public",
): Promise<TableInfo[]> {
  const rows = await sql`
    SELECT
      t.table_schema                                                   AS schema,
      t.table_name                                                     AS name,
      CASE WHEN t.table_type = 'BASE TABLE' THEN 'table' ELSE 'view' END AS type,
      COALESCE(s.n_live_tup, 0)::bigint                                AS estimated_rows,
      COALESCE(pg_total_relation_size(quote_ident(t.table_schema) || '.' || quote_ident(t.table_name)), 0) AS size_bytes,
      pg_size_pretty(
        COALESCE(pg_total_relation_size(quote_ident(t.table_schema) || '.' || quote_ident(t.table_name)), 0)
      )                                                                 AS size,
      EXISTS (
        SELECT 1 FROM information_schema.table_constraints tc
        WHERE tc.table_schema = t.table_schema
          AND tc.table_name = t.table_name
          AND tc.constraint_type = 'PRIMARY KEY'
      )                                                                 AS has_pk
    FROM information_schema.tables t
    LEFT JOIN pg_stat_user_tables s
      ON s.schemaname = t.table_schema AND s.relname = t.table_name
    WHERE t.table_schema = ${schema}
      AND t.table_type IN ('BASE TABLE', 'VIEW')
    ORDER BY t.table_name
  `;

  return rows.map((r) => ({
    schema: r.schema as string,
    name: r.name as string,
    type: r.type as "table" | "view",
    estimated_rows: Number(r.estimated_rows),
    size: r.size as string,
    size_bytes: Number(r.size_bytes),
    has_pk: r.has_pk as boolean,
  }));
}

/**
 * Get detailed column info for a table, including PK, unique, FK, and index info.
 */
export async function getColumns(
  sql: postgres.Sql,
  table: string,
  schema = "public",
): Promise<ColumnInfo[]> {
  const rows = await sql`
    WITH pk_cols AS (
      SELECT kcu.column_name
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      WHERE tc.table_schema = ${schema}
        AND tc.table_name = ${table}
        AND tc.constraint_type = 'PRIMARY KEY'
    ),
    unique_cols AS (
      SELECT DISTINCT kcu.column_name
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      WHERE tc.table_schema = ${schema}
        AND tc.table_name = ${table}
        AND tc.constraint_type = 'UNIQUE'
    ),
    fk_cols AS (
      SELECT
        kcu.column_name,
        ccu.table_schema || '.' || ccu.table_name || '.' || ccu.column_name AS fk_target
      FROM information_schema.table_constraints tc
      JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
        AND tc.table_schema = kcu.table_schema
      JOIN information_schema.constraint_column_usage ccu
        ON tc.constraint_name = ccu.constraint_name
      WHERE tc.table_schema = ${schema}
        AND tc.table_name = ${table}
        AND tc.constraint_type = 'FOREIGN KEY'
    ),
    indexed_cols AS (
      SELECT DISTINCT a.attname AS column_name
      FROM pg_index i
      JOIN pg_class c ON c.oid = i.indrelid
      JOIN pg_namespace n ON n.oid = c.relnamespace
      JOIN pg_attribute a ON a.attrelid = c.oid AND a.attnum = ANY(i.indkey)
      WHERE n.nspname = ${schema}
        AND c.relname = ${table}
    )
    SELECT
      c.column_name                            AS name,
      c.data_type                              AS data_type,
      c.udt_name                               AS udt_name,
      c.ordinal_position::int                  AS ordinal_position,
      (c.is_nullable = 'YES')                  AS nullable,
      c.column_default                         AS column_default,
      (pk.column_name IS NOT NULL)             AS is_pk,
      (uq.column_name IS NOT NULL)             AS is_unique,
      (ix.column_name IS NOT NULL)             AS is_indexed,
      fk.fk_target                             AS fk_target,
      c.character_maximum_length::int          AS max_length
    FROM information_schema.columns c
    LEFT JOIN pk_cols pk ON pk.column_name = c.column_name
    LEFT JOIN unique_cols uq ON uq.column_name = c.column_name
    LEFT JOIN fk_cols fk ON fk.column_name = c.column_name
    LEFT JOIN indexed_cols ix ON ix.column_name = c.column_name
    WHERE c.table_schema = ${schema}
      AND c.table_name = ${table}
    ORDER BY c.ordinal_position
  `;

  return rows.map((r) => ({
    name: r.name as string,
    data_type: r.data_type as string,
    udt_name: r.udt_name as string,
    ordinal_position: r.ordinal_position as number,
    nullable: r.nullable as boolean,
    column_default: r.column_default as string | null,
    is_pk: r.is_pk as boolean,
    is_unique: r.is_unique as boolean,
    is_indexed: r.is_indexed as boolean,
    fk_target: r.fk_target as string | null,
    max_length: r.max_length as number | null,
  }));
}

/**
 * Get the primary key column name(s) for a table.
 * Returns an array (may be composite PK).
 */
export async function getPrimaryKeys(
  sql: postgres.Sql,
  table: string,
  schema = "public",
): Promise<string[]> {
  const rows = await sql`
    SELECT kcu.column_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
      ON tc.constraint_name = kcu.constraint_name
      AND tc.table_schema = kcu.table_schema
    WHERE tc.table_schema = ${schema}
      AND tc.table_name = ${table}
      AND tc.constraint_type = 'PRIMARY KEY'
    ORDER BY kcu.ordinal_position
  `;
  return rows.map((r) => r.column_name as string);
}
