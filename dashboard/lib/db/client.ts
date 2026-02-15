import postgres from "postgres";
import { getConnection } from "./connections";

// ── Per-connection pool cache ──────────────────────────────────────

const pools = new Map<string, postgres.Sql>();
const MAX_POOLS = 20;

/**
 * Get a connection pool for a registered connection ID.
 * Looks up credentials from the Rust backend, creates/caches a pool.
 * Throws if the connection ID is not found.
 */
export async function getPool(connectionId: string): Promise<postgres.Sql> {
  const existing = pools.get(connectionId);
  if (existing) return existing;

  const config = await getConnection(connectionId);
  if (!config) {
    throw new Error(`Connection "${connectionId}" not found. Add it via the Database Manager.`);
  }

  // Evict oldest pool if at capacity
  if (pools.size >= MAX_POOLS) {
    const oldest = pools.keys().next().value;
    if (oldest !== undefined) {
      const pool = pools.get(oldest);
      pools.delete(oldest);
      pool?.end({ timeout: 2 }).catch(() => {});
    }
  }

  const sql = postgres({
    host: config.host,
    port: config.port,
    database: config.database,
    username: config.username,
    password: config.password,
    ssl: config.ssl ? { rejectUnauthorized: false } : false,
    max: 10,
    idle_timeout: 20,
    connect_timeout: 10,
  });

  pools.set(connectionId, sql);
  return sql;
}

/**
 * Invalidate a cached pool (e.g., after connection config changes).
 */
export function invalidatePool(connectionId: string): void {
  const pool = pools.get(connectionId);
  if (pool) {
    pools.delete(connectionId);
    pool.end({ timeout: 2 }).catch(() => {});
  }
}

/**
 * Create a temporary pool for testing a connection. Not cached.
 */
export function createTestPool(opts: {
  host: string;
  port: number;
  database: string;
  username: string;
  password: string;
  ssl: boolean;
}): postgres.Sql {
  return postgres({
    host: opts.host,
    port: opts.port,
    database: opts.database,
    username: opts.username,
    password: opts.password,
    ssl: opts.ssl ? { rejectUnauthorized: false } : false,
    max: 1,
    idle_timeout: 5,
    connect_timeout: 5,
  });
}

/** Graceful shutdown — end all pools. */
export async function closeAll(): Promise<void> {
  const promises: Promise<void>[] = [];
  for (const [, sql] of pools) {
    promises.push(sql.end({ timeout: 5 }));
  }
  pools.clear();
  await Promise.allSettled(promises);
}
