"use client";

// Type-aware cell renderer for PG column types.
// Renders jsonb as collapsed preview, arrays as chips, timestamps as relative, etc.

interface TypeCellProps {
  value: unknown;
  udtName: string;
}

const TYPE_BADGE_COLORS: Record<string, string> = {
  jsonb: "#f472b6",
  json: "#f472b6",
  _text: "#06d6a0",
  _int4: "#06d6a0",
  _int8: "#06d6a0",
  _float8: "#06d6a0",
  vector: "#a855f7",
  bool: "#ffe600",
  timestamptz: "#00d4ff",
  timestamp: "#00d4ff",
  uuid: "#64748b",
  int4: "#00f0ff",
  int8: "#00f0ff",
  float8: "#00f0ff",
  numeric: "#00f0ff",
  text: "#94a3b8",
  varchar: "#94a3b8",
};

export function typeBadgeColor(udtName: string): string {
  return TYPE_BADGE_COLORS[udtName] || "#64748b";
}

export function TypeBadge({ udtName }: { udtName: string }) {
  const color = typeBadgeColor(udtName);
  return (
    <span
      className="text-[8px] font-bold uppercase tracking-wider px-1 py-0.5 rounded ml-1 shrink-0"
      style={{
        color,
        background: `${color}15`,
        border: `1px solid ${color}25`,
      }}
    >
      {udtName}
    </span>
  );
}

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 0) return "in the future";
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.floor(diff / 3_600_000)}h ago`;
  return `${Math.floor(diff / 86_400_000)}d ago`;
}

function truncateJson(val: unknown, maxLen = 60): string {
  const s = JSON.stringify(val);
  if (s.length <= maxLen) return s;
  return s.slice(0, maxLen) + "...";
}

export default function TypeCell({ value, udtName }: TypeCellProps) {
  if (value === null || value === undefined) {
    return <span className="text-slate-600 italic">null</span>;
  }

  // Boolean
  if (udtName === "bool") {
    const boolVal = Boolean(value);
    return (
      <span
        className="text-[10px] font-bold uppercase px-1.5 py-0.5 rounded"
        style={{
          color: boolVal ? "#06d6a0" : "#ff4757",
          background: boolVal ? "rgba(6,214,160,0.12)" : "rgba(255,71,87,0.12)",
        }}
      >
        {boolVal ? "true" : "false"}
      </span>
    );
  }

  // Timestamps
  if (udtName === "timestamptz" || udtName === "timestamp") {
    const iso = String(value);
    return (
      <span className="text-slate-400" title={iso}>
        {relativeTime(iso)}
      </span>
    );
  }

  // JSONB / JSON
  if (udtName === "jsonb" || udtName === "json") {
    return (
      <span
        className="text-pink-300 cursor-default"
        title={JSON.stringify(value, null, 2)}
      >
        {truncateJson(value)}
      </span>
    );
  }

  // Arrays
  if (udtName.startsWith("_") || Array.isArray(value)) {
    const arr = Array.isArray(value) ? value : [value];
    if (arr.length === 0) return <span className="text-slate-600">[]</span>;
    return (
      <span className="flex gap-1 flex-wrap">
        {arr.slice(0, 5).map((item, i) => (
          <span
            key={i}
            className="text-[9px] px-1.5 py-0.5 rounded"
            style={{
              background: "rgba(6,214,160,0.1)",
              color: "#06d6a0",
              border: "1px solid rgba(6,214,160,0.2)",
            }}
          >
            {String(item)}
          </span>
        ))}
        {arr.length > 5 && (
          <span className="text-[9px] text-slate-600">
            +{arr.length - 5}
          </span>
        )}
      </span>
    );
  }

  // UUID
  if (udtName === "uuid") {
    const s = String(value);
    return (
      <span className="text-slate-500 font-mono" title={s}>
        {s.slice(0, 8)}...
      </span>
    );
  }

  // Numbers
  if (
    udtName === "int4" ||
    udtName === "int8" ||
    udtName === "float8" ||
    udtName === "numeric"
  ) {
    return (
      <span className="text-cyan-300 font-mono">
        {typeof value === "number" ? value.toLocaleString() : String(value)}
      </span>
    );
  }

  // Default: text
  const str = String(value);
  if (str.length > 80) {
    return (
      <span className="text-slate-300" title={str}>
        {str.slice(0, 80)}...
      </span>
    );
  }
  return <span className="text-slate-300">{str}</span>;
}
