import type { CooccurrenceData, CooccurrenceEntry } from "@/lib/api";

/**
 * Transform raw API data into CooccurrenceData for the heatmap.
 *
 * Expects: { entity_type_a, entity_type_b, pairs: [{ entity_a, entity_b, count, pmi }] }
 * Falls back to reasonable defaults for missing fields.
 */
export function adaptHeatmapData(raw: unknown): CooccurrenceData {
  const empty: CooccurrenceData = { entity_type_a: "Entity", entity_type_b: "Entity", pairs: [] };

  if (!raw || typeof raw !== "object") return empty;
  const obj = raw as Record<string, unknown>;

  const pairs = Array.isArray(obj.pairs) ? obj.pairs : [];
  const adapted: CooccurrenceEntry[] = pairs
    .filter((p): p is Record<string, unknown> => p !== null && typeof p === "object")
    .map((p) => ({
      entity_a: String(p.entity_a ?? ""),
      entity_b: String(p.entity_b ?? ""),
      count: Number(p.count ?? 0),
      pmi: p.pmi != null ? Number(p.pmi) : null,
    }))
    .filter((p) => p.entity_a && p.entity_b);

  return {
    entity_type_a: String(obj.entity_type_a ?? "Entity"),
    entity_type_b: String(obj.entity_type_b ?? "Entity"),
    pairs: adapted,
  };
}
