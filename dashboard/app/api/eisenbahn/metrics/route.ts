import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

const BROKER_URL =
  process.env.EISENBAHN_METRICS_URL || "http://localhost:9090/metrics";

/** TTL for cached metrics (milliseconds). */
const CACHE_TTL_MS = 1_000;

// ── Dashboard-facing types ───────────────────────────────────────
interface TopicMetrics {
  count: number;
  rate: number;
}

interface WorkerMetrics {
  status: string;
  cpu_pct: number;
  mem_bytes: number;
  last_seen_secs_ago: number;
}

interface TimeSeriesPoint {
  ts: string;
  value: number;
  metric: string;
}

export interface EisenbahnMetrics {
  topics: Record<string, TopicMetrics>;
  workers: Record<string, WorkerMetrics>;
  time_series: TimeSeriesPoint[];
  total_messages: number;
  uptime_secs: number;
  /** True when serving stale data because broker is unreachable. */
  stale?: boolean;
  /** ISO timestamp of when the data was fetched from the broker. */
  fetched_at: string;
}

// ── Broker-facing types (Rust MetricsResponse shape) ─────────────
interface BrokerTopicMetrics {
  total_messages: number;
  total_bytes: number;
  messages_per_sec: number;
  bytes_per_sec: number;
}

interface BrokerWorkerSnapshot {
  worker_id: string;
  status: string;
  cpu_pct: number;
  mem_bytes: number;
  last_seen_secs_ago: number;
}

interface BrokerTimeSeriesPoint {
  elapsed_secs: number;
  total_messages: number;
  topic_rates: Record<string, number>;
}

interface BrokerMetricsResponse {
  topics: Record<string, BrokerTopicMetrics>;
  workers: BrokerWorkerSnapshot[];
  time_series: BrokerTimeSeriesPoint[];
  total_messages: number;
  uptime_secs: number;
}

// ── Transform broker → dashboard ─────────────────────────────────

function transformMetrics(broker: BrokerMetricsResponse): Omit<EisenbahnMetrics, "stale" | "fetched_at"> {
  // Topics: {total_messages, messages_per_sec} → {count, rate}
  const topics: Record<string, TopicMetrics> = {};
  for (const [name, tm] of Object.entries(broker.topics)) {
    topics[name] = {
      count: tm.total_messages,
      rate: tm.messages_per_sec,
    };
  }

  // Workers: Vec<{worker_id, ...}> → Record<worker_id, {...}>
  const workers: Record<string, WorkerMetrics> = {};
  for (const w of broker.workers) {
    workers[w.worker_id] = {
      status: w.status.toLowerCase() === "healthy" ? "online" : w.status.toLowerCase(),
      cpu_pct: w.cpu_pct,
      mem_bytes: w.mem_bytes,
      last_seen_secs_ago: w.last_seen_secs_ago,
    };
  }

  // Time series: flatten topic_rates map into individual {ts, value, metric} points
  const time_series: TimeSeriesPoint[] = [];
  for (const point of broker.time_series) {
    const ts = new Date(Date.now() - (broker.uptime_secs - point.elapsed_secs) * 1000).toISOString();
    for (const [topic, rate] of Object.entries(point.topic_rates)) {
      time_series.push({ ts, value: rate, metric: topic });
    }
  }

  return {
    topics,
    workers,
    time_series,
    total_messages: broker.total_messages,
    uptime_secs: broker.uptime_secs,
  };
}

// ── In-memory cache ──────────────────────────────────────────────
let cached: EisenbahnMetrics | null = null;
let cachedAt = 0;

// ── Route handler ────────────────────────────────────────────────

/**
 * GET /api/eisenbahn/metrics
 *
 * Proxy to the eisenbahn-broker HTTP metrics endpoint.
 * Transforms the Rust MetricsResponse shape into the dashboard's expected format.
 * Caches responses for 1 s to reduce broker load.
 * Returns last-known-good data when the broker is unreachable.
 */
export async function GET(): Promise<Response> {
  const now = Date.now();

  // Serve from cache if still fresh
  if (cached && now - cachedAt < CACHE_TTL_MS) {
    return NextResponse.json(cached);
  }

  try {
    const res = await fetch(BROKER_URL, {
      cache: "no-store",
      signal: AbortSignal.timeout(3_000),
    });

    if (!res.ok) {
      // Broker responded with an error — fall back to stale data if available
      if (cached) {
        return NextResponse.json({ ...cached, stale: true });
      }
      const text = await res.text().catch(() => "");
      return NextResponse.json(
        { error: text || "Broker returned an error" },
        { status: res.status },
      );
    }

    const brokerData = (await res.json()) as BrokerMetricsResponse;
    const transformed = transformMetrics(brokerData);
    const metrics: EisenbahnMetrics = {
      ...transformed,
      stale: false,
      fetched_at: new Date().toISOString(),
    };

    // Update cache
    cached = metrics;
    cachedAt = now;

    return NextResponse.json(metrics);
  } catch (err) {
    // Network error — serve stale cache if we have one
    if (cached) {
      return NextResponse.json({ ...cached, stale: true });
    }

    return NextResponse.json(
      { error: err instanceof Error ? err.message : String(err) },
      { status: 503 },
    );
  }
}
