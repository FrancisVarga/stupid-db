import type { TreemapNode } from "@/components/viz/Treemap";

/**
 * Transform raw API data into a TreemapNode hierarchy.
 *
 * Handles:
 *  1. Already a hierarchy — { name, children: [...] } or { name, value }.
 *  2. Record<string, number> — flat key-value map turned into children.
 *  3. Array of { name/label, value/count } — turned into children.
 */
export function adaptTreemapData(raw: unknown): TreemapNode {
  const root: TreemapNode = { name: "root", children: [] };

  if (!raw) return root;

  // Shape 1: already hierarchical
  if (typeof raw === "object" && !Array.isArray(raw)) {
    const obj = raw as Record<string, unknown>;
    if (obj.children && Array.isArray(obj.children)) {
      return {
        name: String(obj.name ?? "root"),
        children: (obj.children as unknown[]).map(coerceNode).filter(Boolean) as TreemapNode[],
      };
    }

    // Shape 2: flat key-value map
    const entries = Object.entries(obj).filter(([, v]) => typeof v === "number");
    if (entries.length > 0) {
      root.children = entries.map(([name, value]) => ({ name, value: value as number }));
      return root;
    }

    // Check for array wrapper
    for (const key of Object.keys(obj)) {
      if (Array.isArray(obj[key]) && (obj[key] as unknown[]).length > 0) {
        root.children = (obj[key] as unknown[]).map(coerceNode).filter(Boolean) as TreemapNode[];
        return root;
      }
    }
  }

  // Shape 3: flat array
  if (Array.isArray(raw)) {
    root.children = raw.map(coerceNode).filter(Boolean) as TreemapNode[];
    return root;
  }

  return root;
}

function coerceNode(item: unknown): TreemapNode | null {
  if (!item || typeof item !== "object") return null;
  const obj = item as Record<string, unknown>;
  const name = String(obj.name ?? obj.label ?? obj.key ?? "");
  if (!name) return null;

  const children = Array.isArray(obj.children)
    ? (obj.children as unknown[]).map(coerceNode).filter(Boolean) as TreemapNode[]
    : undefined;

  return {
    name,
    value: obj.value != null ? Number(obj.value) : (obj.count != null ? Number(obj.count) : undefined),
    children,
  };
}
