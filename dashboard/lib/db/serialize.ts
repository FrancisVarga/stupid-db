/**
 * Bidirectional serialization between PostgreSQL types and JSON-safe values.
 *
 * postgres.js handles most type coercion automatically, but we need explicit
 * handling for certain types when writing data back to PG, and for ensuring
 * consistent JSON output.
 */

// ── PG udt_name → JSON Schema type mapping ────────────────────────

export interface JsonSchemaType {
  type: string;
  format?: string;
  items?: JsonSchemaType;
  description?: string;
}

const UDT_TO_JSON_SCHEMA: Record<string, JsonSchemaType> = {
  // Booleans
  bool: { type: "boolean" },

  // Integers
  int2: { type: "integer", format: "int32" },
  int4: { type: "integer", format: "int32" },
  int8: { type: "string", format: "int64", description: "BigInt as string to avoid JS precision loss" },

  // Floats
  float4: { type: "number", format: "float" },
  float8: { type: "number", format: "double" },
  numeric: { type: "string", format: "decimal", description: "Arbitrary precision as string" },

  // Strings
  text: { type: "string" },
  varchar: { type: "string" },
  bpchar: { type: "string" }, // char(n)
  name: { type: "string" },

  // Date/time
  timestamp: { type: "string", format: "date-time" },
  timestamptz: { type: "string", format: "date-time" },
  date: { type: "string", format: "date" },
  time: { type: "string", format: "time" },
  timetz: { type: "string", format: "time" },
  interval: { type: "string", format: "duration" },

  // JSON
  json: { type: "object" },
  jsonb: { type: "object" },

  // UUID
  uuid: { type: "string", format: "uuid" },

  // Binary
  bytea: { type: "string", format: "byte", description: "Base64-encoded binary" },

  // Arrays (common ones)
  _text: { type: "array", items: { type: "string" } },
  _varchar: { type: "array", items: { type: "string" } },
  _int4: { type: "array", items: { type: "integer" } },
  _int8: { type: "array", items: { type: "string", format: "int64" } },
  _float4: { type: "array", items: { type: "number" } },
  _float8: { type: "array", items: { type: "number" } },
  _bool: { type: "array", items: { type: "boolean" } },
  _uuid: { type: "array", items: { type: "string", format: "uuid" } },
  _jsonb: { type: "array", items: { type: "object" } },

  // pgvector
  vector: { type: "array", items: { type: "number" }, description: "pgvector embedding" },

  // Network
  inet: { type: "string", format: "ipv4" },
  cidr: { type: "string" },
  macaddr: { type: "string" },

  // Geometric (serialize as string)
  point: { type: "string" },
  line: { type: "string" },
  box: { type: "string" },
  circle: { type: "string" },
  polygon: { type: "string" },

  // Other
  oid: { type: "integer" },
  money: { type: "string" },
  xml: { type: "string", format: "xml" },
  tsvector: { type: "string" },
  tsquery: { type: "string" },
};

/**
 * Map a PG udt_name to a JSON Schema type descriptor.
 */
export function udtToJsonSchema(udtName: string): JsonSchemaType {
  return UDT_TO_JSON_SCHEMA[udtName] ?? { type: "string" };
}

// ── Outbound: PG row → JSON-safe value ─────────────────────────────

/**
 * Serialize a PG row value to a JSON-safe representation.
 * postgres.js handles most types, but we normalize edge cases.
 */
export function serializeValue(value: unknown, udtName: string): unknown {
  if (value === null || value === undefined) return null;

  switch (udtName) {
    case "int8":
    case "numeric":
    case "money":
      // Preserve precision as string
      return String(value);

    case "bytea":
      // Buffer → base64
      if (value instanceof Uint8Array || Buffer.isBuffer(value)) {
        return Buffer.from(value).toString("base64");
      }
      return value;

    case "interval":
      // postgres.js returns an object {years, months, days, hours, minutes, seconds}
      // Serialize to ISO 8601 duration
      if (typeof value === "object" && value !== null) {
        return intervalToIso(value as IntervalParts);
      }
      return String(value);

    case "vector":
      // pgvector returns string "[0.1,0.2,...]" — parse to number array
      if (typeof value === "string") {
        return parseVector(value);
      }
      return value;

    default:
      return value;
  }
}

/**
 * Serialize an entire row, applying type-specific conversions.
 */
export function serializeRow(
  row: Record<string, unknown>,
  columnTypes: Map<string, string>,
): Record<string, unknown> {
  const result: Record<string, unknown> = {};
  for (const [key, val] of Object.entries(row)) {
    const udt = columnTypes.get(key) ?? "text";
    result[key] = serializeValue(val, udt);
  }
  return result;
}

// ── Inbound: JSON value → PG-compatible value ──────────────────────

/**
 * Deserialize a JSON value to a PG-compatible representation for INSERT/UPDATE.
 */
export function deserializeValue(value: unknown, udtName: string): unknown {
  if (value === null || value === undefined) return null;

  switch (udtName) {
    case "jsonb":
    case "json":
      // Ensure objects/arrays are stringified for PG
      if (typeof value === "object") {
        return JSON.stringify(value);
      }
      return value;

    case "bytea":
      // Base64 string → Buffer
      if (typeof value === "string") {
        return Buffer.from(value, "base64");
      }
      return value;

    case "vector":
      // Number array → PG vector string "[0.1,0.2,...]"
      if (Array.isArray(value)) {
        return `[${value.join(",")}]`;
      }
      return value;

    case "int8":
      // Accept string or number, pass as string
      return String(value);

    case "numeric":
      return String(value);

    case "bool":
      if (typeof value === "string") {
        return value === "true" || value === "1";
      }
      return Boolean(value);

    default:
      return value;
  }
}

// ── Helpers ────────────────────────────────────────────────────────

interface IntervalParts {
  years?: number;
  months?: number;
  days?: number;
  hours?: number;
  minutes?: number;
  seconds?: number;
}

function intervalToIso(parts: IntervalParts): string {
  const { years = 0, months = 0, days = 0, hours = 0, minutes = 0, seconds = 0 } = parts;
  let iso = "P";
  if (years) iso += `${years}Y`;
  if (months) iso += `${months}M`;
  if (days) iso += `${days}D`;
  if (hours || minutes || seconds) {
    iso += "T";
    if (hours) iso += `${hours}H`;
    if (minutes) iso += `${minutes}M`;
    if (seconds) iso += `${seconds}S`;
  }
  return iso === "P" ? "PT0S" : iso;
}

function parseVector(str: string): number[] {
  // pgvector format: "[0.1,0.2,0.3]"
  const inner = str.replace(/^\[/, "").replace(/\]$/, "");
  if (!inner) return [];
  return inner.split(",").map(Number);
}
