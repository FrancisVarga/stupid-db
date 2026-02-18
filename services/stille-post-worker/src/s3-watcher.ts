import {
  S3Client,
  ListObjectsV2Command,
  type _Object,
} from "@aws-sdk/client-s3";
import type { Config } from "./config.js";
import { getDb } from "./db.js";
import { executePipeline } from "./orchestrator.js";
import type { SpDataSource } from "./types.js";

// ---------------------------------------------------------------------------
// S3 watch configuration — stored in sp_data_sources.config_json
// ---------------------------------------------------------------------------

interface S3WatchConfig {
  bucket: string;
  prefix?: string;
  region?: string;
  watch?: boolean;
  watchInterval?: number; // seconds, default 300
  triggerPipelineId?: string;
}

// ---------------------------------------------------------------------------
// S3FileWatcher — polls S3 buckets for new files and triggers pipelines
// ---------------------------------------------------------------------------

export class S3FileWatcher {
  private watchers: Map<string, NodeJS.Timeout> = new Map();
  private lastSeen: Map<string, Date> = new Map();
  private polling: Set<string> = new Set();
  private config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  /** Load all S3 data sources with watch=true and start polling each. */
  async start(): Promise<void> {
    const sql = getDb(this.config);

    const sources = await sql<SpDataSource[]>`
      SELECT id, name, source_type, config_json, created_at, updated_at
      FROM sp_data_sources
      WHERE source_type = 's3'
    `;

    let watchCount = 0;
    for (const source of sources) {
      const cfg = source.config_json as unknown as S3WatchConfig;
      if (!cfg.watch || !cfg.triggerPipelineId) continue;

      const intervalSec = cfg.watchInterval ?? 300;
      this.lastSeen.set(source.id, new Date());

      const timer = setInterval(
        () => this.pollBucket(source),
        intervalSec * 1000,
      );
      this.watchers.set(source.id, timer);
      watchCount++;

      console.log(
        `[S3Watcher] Watching s3://${cfg.bucket}/${cfg.prefix ?? ""} every ${intervalSec}s (source: ${source.name})`,
      );
    }

    console.log(`[S3Watcher] Started — ${watchCount} bucket(s) being watched`);
  }

  /** Clear all polling intervals. */
  async stop(): Promise<void> {
    for (const [id, timer] of this.watchers) {
      clearInterval(timer);
    }
    this.watchers.clear();
    this.lastSeen.clear();
    this.polling.clear();
    console.log("[S3Watcher] Stopped");
  }

  // -------------------------------------------------------------------------
  // Private
  // -------------------------------------------------------------------------

  private async pollBucket(dataSource: SpDataSource): Promise<void> {
    // Guard against overlapping polls for the same data source
    if (this.polling.has(dataSource.id)) return;
    this.polling.add(dataSource.id);

    try {
      const cfg = dataSource.config_json as unknown as S3WatchConfig;
      const since = this.lastSeen.get(dataSource.id) ?? new Date();

      const newFiles = await this.listNewFiles(
        cfg.bucket,
        cfg.prefix ?? "",
        since,
        cfg.region,
      );

      if (newFiles.length === 0) return;

      console.log(
        `[S3Watcher] Found ${newFiles.length} new file(s) in s3://${cfg.bucket}/${cfg.prefix ?? ""} (source: ${dataSource.name})`,
      );

      // Update last-seen to now so the next poll won't re-process these
      this.lastSeen.set(dataSource.id, new Date());

      // Trigger the associated pipeline with file metadata as initial input
      await executePipeline(
        cfg.triggerPipelineId!,
        "event",
        this.config,
        undefined,
        {
          trigger: "s3_file_watcher",
          dataSourceId: dataSource.id,
          dataSourceName: dataSource.name,
          bucket: cfg.bucket,
          prefix: cfg.prefix ?? "",
          newFiles,
        },
      );
    } catch (err) {
      console.error(
        `[S3Watcher] Error polling source ${dataSource.name}:`,
        err,
      );
    } finally {
      this.polling.delete(dataSource.id);
    }
  }

  private async listNewFiles(
    bucket: string,
    prefix: string,
    since: Date,
    region?: string,
  ): Promise<string[]> {
    const client = new S3Client({ region: region ?? "eu-central-1" });

    const newFiles: string[] = [];
    let continuationToken: string | undefined;

    do {
      const command = new ListObjectsV2Command({
        Bucket: bucket,
        Prefix: prefix,
        ContinuationToken: continuationToken,
      });

      const response = await client.send(command);

      if (response.Contents) {
        for (const obj of response.Contents) {
          if (obj.LastModified && obj.LastModified > since && obj.Key) {
            newFiles.push(obj.Key);
          }
        }
      }

      continuationToken = response.IsTruncated
        ? response.NextContinuationToken
        : undefined;
    } while (continuationToken);

    client.destroy();
    return newFiles;
  }
}
