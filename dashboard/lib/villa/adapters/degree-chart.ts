import type { DegreeEntry } from "@/lib/api";

/**
 * Transform raw API data into DegreeEntry[].
 *
 * Expects: array of { id, entity_type, key, in_deg, out_deg, total }
 * or object wrapping such an array.
 */
export function adaptDegreeData(raw: unknown): DegreeEntry[] {
  const rows = extractArray(raw);
  return rows.map((item): DegreeEntry => ({
    id: String(item.id ?? ""),
    entity_type: String(item.entity_type ?? "Unknown"),
    key: String(item.key ?? item.label ?? ""),
    in_deg: Number(item.in_deg ?? 0),
    out_deg: Number(item.out_deg ?? 0),
    total: Number(item.total ?? (Number(item.in_deg ?? 0) + Number(item.out_deg ?? 0))),
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
