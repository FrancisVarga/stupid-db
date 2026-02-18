import postgres from "postgres";
import type { Config } from "./config.js";

let sql: postgres.Sql | null = null;

export function getDb(config: Config): postgres.Sql {
  if (!sql) {
    sql = postgres(config.databaseUrl, {
      max: 10,
      idle_timeout: 20,
      connect_timeout: 10,
    });
  }
  return sql;
}

export async function closeDb(): Promise<void> {
  if (sql) {
    await sql.end();
    sql = null;
  }
}
