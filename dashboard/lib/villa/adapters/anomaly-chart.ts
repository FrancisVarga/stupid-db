import type { AnomalyEntry, FeatureDimension } from "@/lib/api";

/**
 * Transform raw API data into AnomalyEntry[].
 *
 * Expects: array of { id, entity_type, key, score, is_anomalous, features?, cluster_id? }
 * or object wrapping such an array.
 */
export function adaptAnomalyData(raw: unknown): AnomalyEntry[] {
  const rows = extractArray(raw);
  return rows.map((item): AnomalyEntry => ({
    id: String(item.id ?? ""),
    entity_type: String(item.entity_type ?? "Unknown"),
    key: String(item.key ?? item.label ?? ""),
    score: Number(item.score ?? 0),
    is_anomalous: Boolean(item.is_anomalous ?? (Number(item.score ?? 0) >= 0.5)),
    features: Array.isArray(item.features)
      ? (item.features as Record<string, unknown>[]).map(
          (f): FeatureDimension => ({
            name: String(f.name ?? ""),
            value: Number(f.value ?? 0),
          }),
        )
      : undefined,
    cluster_id: item.cluster_id != null ? Number(item.cluster_id) : undefined,
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
