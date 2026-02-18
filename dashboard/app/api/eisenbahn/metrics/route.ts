import { NextResponse } from "next/server";

export const dynamic = "force-dynamic";

const BROKER_URL =
  process.env.EISENBAHN_METRICS_URL || "http://localhost:9090/metrics";

/** TTL for cached metrics (milliseconds). */
const CACHE_TTL_MS = 1_000;

// ── In-memory cache ────────────────────────────────────────────
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

let cached: EisenbahnMetrics | null = null;
let cachedAt = 0;

// ── Route handler ──────────────────────────────────────────────

/**
 * GET /api/eisenbahn/metrics
 *
 * Proxy to the eisenbahn-broker HTTP metrics endpoint.
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

    const data = (await res.json()) as Omit<EisenbahnMetrics, "fetched_at">;
    const metrics: EisenbahnMetrics = {
      ...data,
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
