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

export interface DatabaseStats {
  version: string;
  uptime_seconds: number;
  size: string;
  size_bytes: number;
  active_connections: number;
  max_connections: number;
  cache_hit_ratio: number; // 0–1
  total_commits: number;
  total_rollbacks: number;
  dead_tuples: number;
  schema_count: number;
}

/**
 * Get system-level stats for the connected database.
 */
export async function getDatabaseStats(sql: postgres.Sql): Promise<DatabaseStats> {
  const [versionRows, connRows, dbStatRows, deadRows, schemaRows] = await Promise.all([
    sql`SELECT
          version() AS version,
          extract(epoch FROM (now() - pg_postmaster_start_time()))::bigint AS uptime`,
    sql`SELECT
          (SELECT count(*)::int FROM pg_stat_activity WHERE datname = current_database()) AS active,
          current_setting('max_connections')::int AS max_conn`,
    sql`SELECT
          pg_database_size(current_database()) AS size_bytes,
          pg_size_pretty(pg_database_size(current_database())) AS size,
          xact_commit::bigint AS commits,
          xact_rollback::bigint AS rollbacks,
          CASE WHEN blks_hit + blks_read > 0
            THEN round(blks_hit::numeric / (blks_hit + blks_read), 4)
            ELSE 1
          END AS cache_hit
        FROM pg_stat_database WHERE datname = current_database()`,
    sql`SELECT coalesce(sum(n_dead_tup), 0)::bigint AS dead FROM pg_stat_user_tables`,
    sql`SELECT count(*)::int AS cnt FROM information_schema.schemata
        WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
          AND schema_name NOT LIKE 'pg_temp_%' AND schema_name NOT LIKE 'pg_toast_temp_%'`,
  ]);

  const versionRow = versionRows[0];
  const connRow = connRows[0];
  const dbStatRow = dbStatRows[0];
  const deadRow = deadRows[0];
  const schemaRow = schemaRows[0];

  return {
    version: versionRow ? (versionRow.version as string).split(",")[0] : "unknown",
    uptime_seconds: versionRow ? Number(versionRow.uptime) : 0,
    size: dbStatRow?.size as string ?? "0 bytes",
    size_bytes: Number(dbStatRow?.size_bytes ?? 0),
    active_connections: (connRow?.active as number) ?? 0,
    max_connections: (connRow?.max_conn as number) ?? 100,
    cache_hit_ratio: Number(dbStatRow?.cache_hit ?? 1),
    total_commits: Number(dbStatRow?.commits ?? 0),
    total_rollbacks: Number(dbStatRow?.rollbacks ?? 0),
    dead_tuples: Number(deadRow?.dead ?? 0),
    schema_count: (schemaRow?.cnt as number) ?? 0,
  };
}

export interface RealtimeStats {
  ts: number; // epoch ms
  // CPU proxy: active backends and transaction rates
  active_backends: number;
  idle_backends: number;
  waiting_backends: number;
  tps: number; // transactions committed since last reset (cumulative)
  // Memory proxy: shared buffer usage
  blks_hit: number; // cumulative
  blks_read: number; // cumulative
  shared_buffers_mb: number;
  temp_bytes: number; // cumulative temp file bytes
  // I/O: bgwriter + checkpointer
  buffers_checkpoint: number; // cumulative
  buffers_backend: number; // cumulative
  buffers_alloc: number; // cumulative
  // Throughput: tuples in/out
  tup_fetched: number; // cumulative
  tup_inserted: number; // cumulative
  tup_updated: number; // cumulative
  tup_deleted: number; // cumulative
}

/**
 * Lightweight realtime stats for polling (< 5ms query time).
 * Returns cumulative counters — the frontend computes deltas.
 */
export async function getRealtimeStats(sql: postgres.Sql): Promise<RealtimeStats> {
  // Detect PG version for bgwriter compatibility (PG 17 split the view)
  const [{ v }] = await sql`SELECT current_setting('server_version_num')::int AS v`;
  const pgMajor = Math.floor(v / 10000);

  // PG 17+ moved buffers_checkpoint to pg_stat_checkpointer
  const bgWriterQuery =
    pgMajor >= 17
      ? sql`SELECT
              0::bigint AS buf_ckpt,
              0::bigint AS buf_backend,
              buffers_alloc::bigint   AS buf_alloc
            FROM pg_stat_bgwriter`
      : sql`SELECT
              buffers_checkpoint::bigint AS buf_ckpt,
              buffers_backend::bigint    AS buf_backend,
              buffers_alloc::bigint      AS buf_alloc
            FROM pg_stat_bgwriter`;

  const [activityRows, dbStatRows, bgWriterRows, bufRows] = await Promise.all([
    sql`SELECT
          count(*) FILTER (WHERE state = 'active')::int   AS active,
          count(*) FILTER (WHERE state = 'idle')::int      AS idle,
          count(*) FILTER (WHERE wait_event_type IS NOT NULL AND state = 'active')::int AS waiting
        FROM pg_stat_activity
        WHERE datname = current_database()`,
    sql`SELECT
          xact_commit::bigint    AS commits,
          blks_hit::bigint       AS blks_hit,
          blks_read::bigint      AS blks_read,
          temp_bytes::bigint     AS temp_bytes,
          tup_fetched::bigint    AS tup_fetched,
          tup_inserted::bigint   AS tup_inserted,
          tup_updated::bigint    AS tup_updated,
          tup_deleted::bigint    AS tup_deleted
        FROM pg_stat_database
        WHERE datname = current_database()`,
    bgWriterQuery,
    sql`SELECT current_setting('shared_buffers') AS val`,
  ]);

  const activity = activityRows[0];
  const dbStat = dbStatRows[0];
  const bgWriter = bgWriterRows[0];
  const bufSetting = bufRows[0];

  if (!activity || !dbStat || !bgWriter || !bufSetting) {
    throw new Error("Missing rows from PG system views — check permissions");
  }

  return {
    ts: Date.now(),
    active_backends: activity.active as number,
    idle_backends: activity.idle as number,
    waiting_backends: activity.waiting as number,
    tps: Number(dbStat.commits),
    blks_hit: Number(dbStat.blks_hit),
    blks_read: Number(dbStat.blks_read),
    shared_buffers_mb: parseSharedBuffers(bufSetting.val as string),
    temp_bytes: Number(dbStat.temp_bytes),
    buffers_checkpoint: Number(bgWriter.buf_ckpt),
    buffers_backend: Number(bgWriter.buf_backend),
    buffers_alloc: Number(bgWriter.buf_alloc),
    tup_fetched: Number(dbStat.tup_fetched),
    tup_inserted: Number(dbStat.tup_inserted),
    tup_updated: Number(dbStat.tup_updated),
    tup_deleted: Number(dbStat.tup_deleted),
  };
}

/** Parse shared_buffers setting (e.g. "128MB", "1GB", "16384") into MB. */
function parseSharedBuffers(val: string): number {
  const num = parseInt(val, 10);
  if (val.endsWith("GB")) return num * 1024;
  if (val.endsWith("MB")) return num;
  if (val.endsWith("kB")) return Math.round(num / 1024);
  // Raw 8kB pages
  return Math.round((num * 8) / 1024);
}

/**
 * List non-system schemas in a database.
 */
export async function listSchemas(sql: postgres.Sql): Promise<string[]> {
  const rows = await sql`
    SELECT schema_name
    FROM information_schema.schemata
    WHERE schema_name NOT IN ('pg_catalog', 'information_schema', 'pg_toast')
      AND schema_name NOT LIKE 'pg_temp_%'
      AND schema_name NOT LIKE 'pg_toast_temp_%'
      AND schema_name NOT LIKE '_timescaledb_%'
      AND schema_name NOT LIKE 'timescaledb_%'
    ORDER BY
      CASE WHEN schema_name = 'public' THEN 0 ELSE 1 END,
      schema_name
  `;
  return rows.map((r) => r.schema_name as string);
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
      COALESCE(
        NULLIF(s.n_live_tup, 0),
        c.reltuples::bigint,
        0
      )::bigint                                                        AS estimated_rows,
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
    LEFT JOIN pg_namespace n
      ON n.nspname = t.table_schema
    LEFT JOIN pg_class c
      ON c.relname = t.table_name AND c.relnamespace = n.oid
    WHERE t.table_schema = ${schema}
      AND t.table_type IN ('BASE TABLE', 'VIEW')
      AND t.table_name NOT LIKE '_hyper_%_chunk'
      AND t.table_name NOT LIKE '_compressed_hypertable_%'
      AND t.table_name NOT LIKE '_materialized_hypertable_%'
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
