import express from "express";
import { loadConfig } from "./config.js";
import { getDb, closeDb } from "./db.js";
import { SchedulePoller } from "./scheduler.js";
import { S3FileWatcher } from "./s3-watcher.js";

const config = loadConfig();
const app = express();
const sql = getDb(config);
const poller = new SchedulePoller(config);
const s3Watcher = new S3FileWatcher(config);

app.get("/health", async (_req, res) => {
  try {
    await sql`SELECT 1`;
    res.json({ status: "ok", timestamp: new Date().toISOString() });
  } catch (err) {
    res.status(503).json({
      status: "error",
      error: err instanceof Error ? err.message : "unknown",
    });
  }
});

const server = app.listen(config.port, () => {
  console.log(`[stille-post-worker] listening on port ${config.port}`);
  console.log(`[stille-post-worker] API base: ${config.apiBase}`);
  poller.start();
});

async function shutdown() {
  console.log("[stille-post-worker] shutting down...");
  poller.stop();
  server.close();
  await closeDb();
  process.exit(0);
}

process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
