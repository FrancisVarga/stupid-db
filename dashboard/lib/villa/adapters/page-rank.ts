import type { PageRankEntry } from "@/lib/api";

/**
 * Transform raw API data into PageRankEntry[].
 *
 * Expects: array of { id, entity_type, key, score }
 * or object wrapping such an array.
 */
export function adaptPageRankData(raw: unknown): PageRankEntry[] {
  const rows = extractArray(raw);
  return rows.map((item): PageRankEntry => ({
    id: String(item.id ?? ""),
    entity_type: String(item.entity_type ?? "Unknown"),
    key: String(item.key ?? item.label ?? ""),
    score: Number(item.score ?? 0),
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
