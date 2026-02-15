"use client";

import { useEffect, useRef } from "react";
import * as d3 from "d3";

// ── Insight Types ──────────────────────────────────────────

export interface MiniViz {
  type: "sparkline" | "mini_bar" | "badge";
  data: number[] | { value: string; color: string };
}

export interface Insight {
  id: string;
  type: "anomaly" | "trend" | "pattern" | "cluster" | "graph";
  severity: "info" | "warning" | "critical";
  title: string;
  description: string;
  timestamp: string;
  miniViz?: MiniViz;
  drillDownQuery?: string;
}

export interface SystemStatus {
  compute_active: boolean;
  segments_active: number;
  segments_total: number;
  anomaly_count: number;
}

// ── Severity Colors ──────────────────────────────────────

const SEVERITY_COLORS = {
  info: "#00f0ff",
  warning: "#ff8a00",
  critical: "#ff4757",
};

const TYPE_ICONS: Record<string, string> = {
  anomaly: "\u26A0",
  trend: "\u2191",
  pattern: "\u2B50",
  cluster: "\u25CF",
  graph: "\u25C6",
};

// ── Mini Visualization Components ─────────────────────────

function Sparkline({ data }: { data: number[] }) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || !data.length) return;
    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const w = 80;
    const h = 20;
    svg.attr("width", w).attr("height", h);

    const x = d3
      .scaleLinear()
      .domain([0, data.length - 1])
      .range([2, w - 2]);
    const y = d3
      .scaleLinear()
      .domain(d3.extent(data) as [number, number])
      .range([h - 2, 2]);

    const line = d3
      .line<number>()
      .x((_, i) => x(i))
      .y((d) => y(d))
      .curve(d3.curveMonotoneX);

    svg
      .append("path")
      .datum(data)
      .attr("fill", "none")
      .attr("stroke", "#00f0ff")
      .attr("stroke-width", 1.5)
      .attr("d", line);
  }, [data]);

  return <svg ref={svgRef} className="inline-block" />;
}

function MiniBar({ data }: { data: number[] }) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || !data.length) return;
    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const w = 80;
    const h = 20;
    svg.attr("width", w).attr("height", h);

    const barW = Math.max(2, (w - data.length) / data.length);
    const maxVal = d3.max(data) || 1;
    const y = d3.scaleLinear().domain([0, maxVal]).range([0, h - 2]);

    svg
      .selectAll("rect")
      .data(data)
      .join("rect")
      .attr("x", (_, i) => i * (barW + 1))
      .attr("y", (d) => h - y(d))
      .attr("width", barW)
      .attr("height", (d) => y(d))
      .attr("rx", 1)
      .attr("fill", "#00f0ff60");
  }, [data]);

  return <svg ref={svgRef} className="inline-block" />;
}

function Badge({ value, color }: { value: string; color: string }) {
  return (
    <span
      className="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold font-mono"
      style={{
        background: `${color}15`,
        color: color,
        border: `1px solid ${color}30`,
      }}
    >
      {value}
    </span>
  );
}

function MiniVizRenderer({ miniViz }: { miniViz: MiniViz }) {
  if (miniViz.type === "sparkline" && Array.isArray(miniViz.data)) {
    return <Sparkline data={miniViz.data} />;
  }
  if (miniViz.type === "mini_bar" && Array.isArray(miniViz.data)) {
    return <MiniBar data={miniViz.data} />;
  }
  if (
    miniViz.type === "badge" &&
    !Array.isArray(miniViz.data) &&
    miniViz.data
  ) {
    return <Badge value={miniViz.data.value} color={miniViz.data.color} />;
  }
  return null;
}

// ── Insight Card ──────────────────────────────────────────

function InsightItem({
  insight,
  onClick,
  onDismiss,
}: {
  insight: Insight;
  onClick?: () => void;
  onDismiss?: () => void;
}) {
  const color = SEVERITY_COLORS[insight.severity];
  const icon = TYPE_ICONS[insight.type] || "\u25CB";

  return (
    <div
      className="rounded-lg p-2.5 relative overflow-hidden transition-all group"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${color}15`,
        cursor: onClick ? "pointer" : "default",
      }}
      onClick={onClick}
    >
      {/* Top accent */}
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${color}40, transparent)`,
        }}
      />

      <div className="flex items-start justify-between gap-2">
        <div className="flex items-start gap-2 min-w-0">
          <span
            className="shrink-0 text-xs mt-0.5"
            style={{ color }}
          >
            {icon}
          </span>
          <div className="min-w-0">
            <div
              className="text-[10px] font-bold tracking-wider uppercase truncate"
              style={{ color }}
            >
              {insight.title}
            </div>
            <div className="text-[10px] text-slate-500 mt-0.5 line-clamp-2">
              {insight.description}
            </div>
          </div>
        </div>
        {onDismiss && (
          <button
            onClick={(e) => {
              e.stopPropagation();
              onDismiss();
            }}
            className="text-slate-700 hover:text-slate-500 text-xs opacity-0 group-hover:opacity-100 transition-opacity shrink-0"
          >
            &times;
          </button>
        )}
      </div>

      {insight.miniViz && (
        <div className="mt-1.5">
          <MiniVizRenderer miniViz={insight.miniViz} />
        </div>
      )}

      <div className="text-[9px] text-slate-700 font-mono mt-1">
        {new Date(insight.timestamp).toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        })}
      </div>
    </div>
  );
}

// ── System Status Widget ─────────────────────────────────

function SystemStatusWidget({ status }: { status: SystemStatus }) {
  return (
    <div
      className="rounded-lg p-2.5"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: "1px solid rgba(100, 116, 139, 0.1)",
      }}
    >
      <div className="text-[9px] text-slate-600 uppercase tracking-widest font-bold mb-2">
        System Status
      </div>
      <div className="space-y-1.5">
        <StatusRow
          label="Compute"
          value={status.compute_active ? "active" : "idle"}
          color={status.compute_active ? "#06d6a0" : "#64748b"}
        />
        <StatusRow
          label="Segments"
          value={`${status.segments_active}/${status.segments_total}`}
          color="#00f0ff"
        />
        <StatusRow
          label="Anomalies"
          value={String(status.anomaly_count)}
          color={status.anomaly_count > 0 ? "#ff8a00" : "#64748b"}
        />
      </div>
    </div>
  );
}

function StatusRow({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div className="flex items-center justify-between">
      <div className="flex items-center gap-1.5">
        <div
          className="w-1.5 h-1.5 rounded-full"
          style={{ background: color }}
        />
        <span className="text-[10px] text-slate-500">{label}</span>
      </div>
      <span className="text-[10px] font-mono" style={{ color }}>
        {value}
      </span>
    </div>
  );
}

// ── Main Sidebar ─────────────────────────────────────────

interface Props {
  insights: Insight[];
  systemStatus: SystemStatus | null;
  onInsightClick?: (query: string) => void;
  onDismissInsight?: (id: string) => void;
}

export default function InsightSidebar({
  insights,
  systemStatus,
  onInsightClick,
  onDismissInsight,
}: Props) {
  return (
    <div className="flex flex-col h-full overflow-hidden">
      {/* Header */}
      <div
        className="px-3 py-2 shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <div className="flex items-center justify-between">
          <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold">
            Insights
          </span>
          {insights.length > 0 && (
            <span
              className="text-[10px] font-mono font-bold px-1.5 py-0.5 rounded-full"
              style={{
                background: "rgba(0, 240, 255, 0.08)",
                color: "#00f0ff",
              }}
            >
              {insights.length}
            </span>
          )}
        </div>
      </div>

      {/* Insight list */}
      <div className="flex-1 overflow-y-auto px-2 py-2 space-y-2">
        {insights.length === 0 && (
          <div className="text-[10px] text-slate-700 text-center py-4">
            No active insights
          </div>
        )}
        {insights.map((insight) => (
          <InsightItem
            key={insight.id}
            insight={insight}
            onClick={
              insight.drillDownQuery
                ? () => onInsightClick?.(insight.drillDownQuery!)
                : undefined
            }
            onDismiss={() => onDismissInsight?.(insight.id)}
          />
        ))}
      </div>

      {/* System status */}
      {systemStatus && (
        <div
          className="px-2 py-2 shrink-0"
          style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          <SystemStatusWidget status={systemStatus} />
        </div>
      )}
    </div>
  );
}
