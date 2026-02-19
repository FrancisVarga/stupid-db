import type { TrendEntry } from "@/lib/api";

/**
 * Transform raw API data into TrendEntry[].
 *
 * Expects: array of { metric, current_value, baseline_mean, direction, magnitude }
 * or object wrapping such an array.
 */
export function adaptTrendData(raw: unknown): TrendEntry[] {
  const rows = extractArray(raw);
  return rows.map((item): TrendEntry => ({
    metric: String(item.metric ?? item.name ?? ""),
    current_value: Number(item.current_value ?? 0),
    baseline_mean: Number(item.baseline_mean ?? 0),
    direction: String(item.direction ?? "Stable"),
    magnitude: Number(item.magnitude ?? 0),
  }));
}

function extractArray(raw: unknown): Record<string, unknown>[] {
  if (Array.isArray(raw)) {
    return raw.filter((item): item is Record<string, unknown> => item !== null && typeof item === "object");
  }
  if (raw !== null && typeof raw === "object") {
    const obj = raw as Record<string, unknown>;
    for (const key of Object.keys(obj)) {
      if (Array.isArray(obj[key]) && (obj[key] as unknown[]).length > 0) {
        return (obj[key] as unknown[]).filter(
          (item): item is Record<string, unknown> => item !== null && typeof item === "object",
        );
      }
    }
  }
  return [];
}
