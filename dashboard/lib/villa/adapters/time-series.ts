import type { TrendEntry } from "@/lib/api";

/** Chart-ready data point for the TimeSeriesChart widget. */
export interface TimeSeriesPoint {
  timestamp: number;
  value: number;
  series?: string;
}

/**
 * Transform raw API response into an array of time series points.
 *
 * Handles two shapes:
 *  1. TrendEntry[] from `/compute/trends` — each entry becomes a point with
 *     metric as the series name, current_value as value, and a synthetic
 *     index-based timestamp (trend data has no real timestamps).
 *  2. An array of objects with { timestamp, value, series? } already in the
 *     expected shape (passthrough).
 */
export function adaptTimeSeriesData(raw: unknown): TimeSeriesPoint[] {
  if (!Array.isArray(raw) || raw.length === 0) return [];

  const first = raw[0] as Record<string, unknown>;

  // Shape 1: TrendEntry[] — has `metric` and `current_value`
  if ("metric" in first && "current_value" in first) {
    const now = Date.now();
    return (raw as TrendEntry[]).map((entry, i) => ({
      timestamp: now - (raw.length - 1 - i) * 60_000, // synthetic 1-min intervals
      value: (entry.current_value as number) ?? 0,
      series: entry.metric,
    }));
  }

  // Shape 2: already { timestamp, value, series? }
  return raw.map((item: Record<string, unknown>) => ({
    timestamp: typeof item.timestamp === "number"
      ? item.timestamp
      : typeof item.timestamp === "string"
        ? new Date(item.timestamp as string).getTime()
        : 0,
    value: (item.value as number) ?? 0,
    series: item.series as string | undefined,
  }));
}
