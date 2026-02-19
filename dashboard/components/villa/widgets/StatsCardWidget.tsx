"use client";

import type { Stats } from "@/lib/api";

interface StatsCardWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

interface MetricCard {
  label: string;
  key: keyof Stats;
  color: string;
}

const METRICS: MetricCard[] = [
  { label: "Documents", key: "doc_count", color: "#00d4ff" },
  { label: "Nodes", key: "node_count", color: "#ff00ff" },
  { label: "Edges", key: "edge_count", color: "#00ff88" },
  { label: "Segments", key: "segment_count", color: "#ffe600" },
];

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

export default function StatsCardWidget({ data, dimensions }: StatsCardWidgetProps) {
  const stats = data as Stats | undefined;
  if (!stats) return null;

  const cols = dimensions.width < 300 ? 2 : dimensions.width < 500 ? 3 : 4;

  return (
    <div
      className="grid gap-2 p-3 h-full content-start"
      style={{ gridTemplateColumns: `repeat(${cols}, 1fr)` }}
    >
      {METRICS.map(({ label, key, color }) => {
        const value = (stats[key] as number) ?? 0;
        return (
          <div
            key={key}
            className="rounded-lg px-3 py-2.5 flex flex-col gap-1"
            style={{
              background: "rgba(255,255,255,0.03)",
              borderLeft: `3px solid ${color}`,
            }}
          >
            <span className="text-[10px] uppercase tracking-wider text-slate-500 font-medium">
              {label}
            </span>
            <span
              className="text-lg font-bold tabular-nums"
              style={{ color }}
            >
              {formatNumber(value)}
            </span>
          </div>
        );
      })}
    </div>
  );
}
