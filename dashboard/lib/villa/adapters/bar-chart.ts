import type { BarChartDatum } from "@/components/viz/BarChart";

/**
 * Transform raw API data into BarChartDatum[].
 *
 * Handles:
 *  1. Array of { label, value } objects (direct match).
 *  2. Object with an array-valued key containing { label/name/key, value/count } objects.
 *  3. Record<string, number> — keys become labels, values become values.
 */
export function adaptBarChartData(raw: unknown): BarChartDatum[] {
  if (!raw) return [];

  // Shape 1: direct array
  if (Array.isArray(raw)) {
    return raw
      .filter((item): item is Record<string, unknown> => item !== null && typeof item === "object")
      .map(toBarDatum)
      .filter((d): d is BarChartDatum => d !== null);
  }

  if (typeof raw !== "object") return [];
  const obj = raw as Record<string, unknown>;

  // Shape 2: object wrapping an array
  for (const key of Object.keys(obj)) {
    if (Array.isArray(obj[key]) && (obj[key] as unknown[]).length > 0) {
      return (obj[key] as unknown[])
        .filter((item): item is Record<string, unknown> => item !== null && typeof item === "object")
        .map(toBarDatum)
        .filter((d): d is BarChartDatum => d !== null);
    }
  }

  // Shape 3: flat key-value map
  const entries = Object.entries(obj).filter(([, v]) => typeof v === "number");
  if (entries.length > 0) {
    return entries.map(([label, value]) => ({ label, value: value as number }));
  }

  return [];
}

function toBarDatum(item: Record<string, unknown>): BarChartDatum | null {
  const label = String(item.label ?? item.name ?? item.key ?? "");
  const value = Number(item.value ?? item.count ?? 0);
  if (!label || isNaN(value)) return null;
  return { label, value, color: item.color as string | undefined };
}
