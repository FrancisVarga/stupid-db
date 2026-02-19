// Villa error handling — runtime validation for LLM-generated configs and actions.

import type { WidgetConfig, WidgetType, LayoutAction } from "./types";

const KNOWN_WIDGET_TYPES: ReadonlySet<string> = new Set<WidgetType>([
  "stats-card",
  "time-series",
  "data-table",
  "force-graph",
]);

const ALLOWED_ENDPOINT_PREFIXES = ["/api/", "/stats", "/compute/"] as const;

function isValidEndpoint(endpoint: unknown): endpoint is string {
  if (typeof endpoint !== "string" || endpoint.length === 0) return false;
  return ALLOWED_ENDPOINT_PREFIXES.some((p) => endpoint.startsWith(p));
}

function isNumberOrUndefined(v: unknown): boolean {
  return v === undefined || typeof v === "number";
}

/**
 * Validate a widget config from an LLM response at runtime.
 * Returns the config if valid, null otherwise.
 */
export function validateWidgetConfig(config: unknown): WidgetConfig | null {
  if (!config || typeof config !== "object") {
    console.error("[villa] invalid widget config: not an object", config);
    return null;
  }

  const c = config as Record<string, unknown>;

  if (typeof c.id !== "string" || c.id.length === 0) {
    console.error("[villa] invalid widget config: missing or empty id", c);
    return null;
  }

  if (typeof c.type !== "string" || !KNOWN_WIDGET_TYPES.has(c.type)) {
    console.error("[villa] invalid widget config: unknown type", c.type);
    return null;
  }

  if (typeof c.title !== "string" || c.title.length === 0) {
    console.error("[villa] invalid widget config: missing title", c);
    return null;
  }

  // Validate dataSource
  const ds = c.dataSource;
  if (!ds || typeof ds !== "object") {
    console.error("[villa] invalid widget config: missing dataSource", c);
    return null;
  }
  const dataSource = ds as Record<string, unknown>;
  if (dataSource.type !== "api" && dataSource.type !== "websocket") {
    console.error("[villa] invalid dataSource type", dataSource.type);
    return null;
  }
  if (!isValidEndpoint(dataSource.endpoint)) {
    console.error("[villa] invalid dataSource endpoint", dataSource.endpoint);
    return null;
  }

  // Validate layout positions are numbers
  const layout = c.layout;
  if (!layout || typeof layout !== "object") {
    console.error("[villa] invalid widget config: missing layout", c);
    return null;
  }
  const l = layout as Record<string, unknown>;
  if (
    typeof l.x !== "number" ||
    typeof l.y !== "number" ||
    typeof l.w !== "number" ||
    typeof l.h !== "number"
  ) {
    console.error("[villa] invalid layout: positions must be numbers", l);
    return null;
  }
  if (!isNumberOrUndefined(l.minW) || !isNumberOrUndefined(l.minH)) {
    console.error("[villa] invalid layout: minW/minH must be numbers", l);
    return null;
  }

  return config as WidgetConfig;
}

const KNOWN_ACTIONS: ReadonlySet<string> = new Set(["add", "remove", "resize", "move"]);

/**
 * Filter a list of layout actions, dropping invalid entries.
 * Logs each rejected action to console.error for debugging.
 */
export function validateLayoutActions(actions: unknown): LayoutAction[] {
  if (!Array.isArray(actions)) {
    console.error("[villa] layout actions is not an array", actions);
    return [];
  }

  return actions.filter((item): item is LayoutAction => {
    if (!item || typeof item !== "object") {
      console.error("[villa] action is not an object", item);
      return false;
    }

    const a = item as Record<string, unknown>;

    if (typeof a.action !== "string" || !KNOWN_ACTIONS.has(a.action)) {
      console.error("[villa] unknown action type", a.action);
      return false;
    }

    // "add" actions must carry a valid widget config
    if (a.action === "add") {
      if (!validateWidgetConfig(a.widget)) {
        console.error("[villa] 'add' action has invalid widget", a.widget);
        return false;
      }
    }

    // "resize" must have numeric dimensions
    if (a.action === "resize") {
      const dims = a.dimensions as Record<string, unknown> | undefined;
      if (!dims || typeof dims.w !== "number" || typeof dims.h !== "number") {
        console.error("[villa] 'resize' action has invalid dimensions", dims);
        return false;
      }
    }

    // "remove", "resize", "move" need a widgetId
    if (a.action !== "add" && typeof a.widgetId !== "string") {
      console.error("[villa] action missing widgetId", a);
      return false;
    }

    return true;
  });
}
