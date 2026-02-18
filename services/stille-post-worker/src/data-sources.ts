import { readFile } from "fs/promises";
import type { Config } from "./config.js";
import { getDb } from "./db.js";
import type { SpDataSource } from "./types.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface FetchResult {
  data: Record<string, unknown>[];
  columns: string[];
  rowCount: number;
  metadata: Record<string, unknown>;
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/**
 * Fetch data from a data source by its ID.
 * Looks up the source in sp_data_sources and dispatches to the appropriate fetcher.
 */
export async function fetchDataSource(
  dataSourceId: string,
  config: Config,
): Promise<FetchResult> {
  const sql = getDb(config);

  const sources = await sql<SpDataSource[]>`
    SELECT id, name, source_type, config_json, created_at, updated_at
    FROM sp_data_sources
    WHERE id = ${dataSourceId}
  `;

  if (sources.length === 0) {
    throw new Error(`Data source not found: ${dataSourceId}`);
  }

  const source = sources[0];

  switch (source.source_type) {
    case "athena":
      return fetchAthena(source.config_json, config);
    case "s3":
      return fetchS3(source.config_json);
    case "api":
      return fetchApi(source.config_json);
    case "upload":
      return fetchUpload(source.config_json);
    default:
      throw new Error(`Unknown source type: ${(source as any).source_type}`);
  }
}

// ---------------------------------------------------------------------------
// Athena fetcher — executes SQL query against Postgres
// ---------------------------------------------------------------------------

async function fetchAthena(
  configJson: Record<string, unknown>,
  config: Config,
): Promise<FetchResult> {
  const query = configJson.query as string | undefined;
  if (!query) {
    throw new Error("Athena data source requires a 'query' field in config_json");
  }

  const sql = getDb(config);

  // Use unsafe() because the query comes from user-configured data sources,
  // not from untrusted external input — these are admin-authored queries.
  const rows = await sql.unsafe(query);

  const columns =
    rows.length > 0 ? Object.keys(rows[0]) : [];

  return {
    data: rows as Record<string, unknown>[],
    columns,
    rowCount: rows.length,
    metadata: { source: "athena", query },
  };
}

// ---------------------------------------------------------------------------
// S3 fetcher — placeholder
// ---------------------------------------------------------------------------

async function fetchS3(
  _configJson: Record<string, unknown>,
): Promise<FetchResult> {
  // TODO: Implement S3 fetching with AWS SDK (@aws-sdk/client-s3)
  // Expected config_json shape: { bucket: string, key: string, format: "csv" | "json" | "parquet" }
  throw new Error(
    "S3 fetcher not yet implemented — requires @aws-sdk/client-s3 dependency",
  );
}

// ---------------------------------------------------------------------------
// API fetcher — fetch from external HTTP endpoint
// ---------------------------------------------------------------------------

async function fetchApi(
  configJson: Record<string, unknown>,
): Promise<FetchResult> {
  const url = configJson.url as string | undefined;
  if (!url) {
    throw new Error("API data source requires a 'url' field in config_json");
  }

  const method = (configJson.method as string) ?? "GET";
  const headers = (configJson.headers as Record<string, string>) ?? {};
  const body = configJson.body as string | undefined;

  const response = await fetch(url, {
    method,
    headers: { "Content-Type": "application/json", ...headers },
    body: method !== "GET" ? body : undefined,
  });

  if (!response.ok) {
    throw new Error(
      `API fetch failed: ${response.status} ${response.statusText} (${url})`,
    );
  }

  const json = await response.json();

  // Normalize: the response might be an array or an object with a data key
  const rows: Record<string, unknown>[] = Array.isArray(json)
    ? json
    : Array.isArray(json.data)
      ? json.data
      : [json];

  const columns = rows.length > 0 ? Object.keys(rows[0]) : [];

  return {
    data: rows,
    columns,
    rowCount: rows.length,
    metadata: { source: "api", url, method, status: response.status },
  };
}

// ---------------------------------------------------------------------------
// Upload fetcher — read file from local storage
// ---------------------------------------------------------------------------

async function fetchUpload(
  configJson: Record<string, unknown>,
): Promise<FetchResult> {
  const filePath = configJson.file_path as string | undefined;
  if (!filePath) {
    throw new Error("Upload data source requires a 'file_path' field in config_json");
  }

  const format = (configJson.format as string) ?? "json";

  const raw = await readFile(filePath, "utf-8");

  let rows: Record<string, unknown>[];

  switch (format) {
    case "json": {
      const parsed = JSON.parse(raw);
      rows = Array.isArray(parsed) ? parsed : [parsed];
      break;
    }
    case "csv": {
      rows = parseCsv(raw);
      break;
    }
    default:
      throw new Error(`Unsupported upload format: ${format} (supported: json, csv)`);
  }

  const columns = rows.length > 0 ? Object.keys(rows[0]) : [];

  return {
    data: rows,
    columns,
    rowCount: rows.length,
    metadata: { source: "upload", filePath, format },
  };
}

// ---------------------------------------------------------------------------
// Minimal CSV parser (no external deps)
// ---------------------------------------------------------------------------

function parseCsv(raw: string): Record<string, unknown>[] {
  const lines = raw.split("\n").filter((l) => l.trim().length > 0);
  if (lines.length < 2) return [];

  const headers = lines[0].split(",").map((h) => h.trim());

  return lines.slice(1).map((line) => {
    const values = line.split(",").map((v) => v.trim());
    const row: Record<string, unknown> = {};
    for (let i = 0; i < headers.length; i++) {
      row[headers[i]] = values[i] ?? null;
    }
    return row;
  });
}
