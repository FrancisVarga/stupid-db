import type { ScatterDatum } from "@/components/viz/ScatterPlot";

/**
 * Transform raw API data into ScatterDatum[].
 *
 * Handles:
 *  1. Array of { x, y } objects.
 *  2. Object wrapping an array with { x, y } objects.
 */
export function adaptScatterPlotData(raw: unknown): ScatterDatum[] {
  const rows = extractArray(raw);
  return rows
    .map((item): ScatterDatum | null => {
      const x = Number(item.x);
      const y = Number(item.y);
      if (isNaN(x) || isNaN(y)) return null;
      return {
        x,
        y,
        label: item.label != null ? String(item.label) : undefined,
        cluster: item.cluster != null ? Number(item.cluster) : undefined,
        size: item.size != null ? Number(item.size) : undefined,
      };
    })
    .filter((d): d is ScatterDatum => d !== null);
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
