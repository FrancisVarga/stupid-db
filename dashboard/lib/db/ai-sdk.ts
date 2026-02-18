import postgres from "postgres";

// ── AI SDK dedicated Postgres client ─────────────────────────────────
// Singleton connection pool for AI SDK tables (chat_sessions, chat_messages).
// Separate from user-registered DB connections managed in client.ts.

const AI_SDK_DATABASE_URL =
  process.env.AI_SDK_DATABASE_URL ??
  "postgresql://localhost:5432/stupid_db";

const sql = postgres(AI_SDK_DATABASE_URL, {
  max: 10,
  idle_timeout: 20,
  connect_timeout: 10,
});

export default sql;
