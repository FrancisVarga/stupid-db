"use client";

interface MetricItem {
  label: string;
  value: string | number;
  change?: string;
}

interface Props {
  title: string;
  text: string;
  metrics?: MetricItem[];
  severity?: "info" | "warning" | "critical";
}

const SEVERITY_COLORS = {
  info: "#00f0ff",
  warning: "#ff8a00",
  critical: "#ff4757",
};

export default function InsightCard({
  title,
  text,
  metrics,
  severity = "info",
}: Props) {
  const color = SEVERITY_COLORS[severity];

  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${color}20`,
        boxShadow: `0 0 20px ${color}05`,
      }}
    >
      {/* Top accent */}
      <div
        className="absolute top-0 left-0 w-full h-[2px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${color}60, transparent)`,
        }}
      />

      {/* Severity dot + title */}
      <div className="flex items-center gap-2 mb-2">
        <div
          className="w-2 h-2 rounded-full shrink-0"
          style={{ background: color, boxShadow: `0 0 6px ${color}60` }}
        />
        <h3
          className="text-xs font-bold tracking-wider uppercase"
          style={{ color }}
        >
          {title}
        </h3>
      </div>

      {/* Text */}
      <p className="text-sm text-slate-400 leading-relaxed">{text}</p>

      {/* Metrics */}
      {metrics && metrics.length > 0 && (
        <div className="flex flex-wrap gap-4 mt-3">
          {metrics.map((m, i) => (
            <div key={i}>
              <div className="text-[10px] text-slate-600 uppercase tracking-widest">
                {m.label}
              </div>
              <div className="flex items-center gap-1.5 mt-0.5">
                <span className="text-sm font-bold font-mono text-slate-200">
                  {typeof m.value === "number"
                    ? m.value.toLocaleString()
                    : m.value}
                </span>
                {m.change && (
                  <span
                    className="text-[10px] font-mono font-bold"
                    style={{
                      color: m.change.startsWith("+")
                        ? "#06d6a0"
                        : m.change.startsWith("-")
                        ? "#ff4757"
                        : "#64748b",
                    }}
                  >
                    {m.change}
                  </span>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
