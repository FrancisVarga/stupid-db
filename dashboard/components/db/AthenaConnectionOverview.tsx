"use client";

import { useEffect, useState, useRef } from "react";
import * as d3 from "d3";
import {
  getAthenaQueryLog,
  type AthenaConnectionSafe,
  type AthenaSchema,
  type DailyCostSummary,
  type AthenaQueryLogEntry,
} from "@/lib/db/athena-connections";

// ── Types ──────────────────────────────────────────────────────────

interface ConnectionStats {
  databases: number;
  tables: number;
  columns: number;
  totalQueries: number;
  totalCostUsd: number;
  totalBytesScanned: number;
  avgCostPerQuery: number;
  avgBytesPerQuery: number;
}

interface SourceBreakdown {
  source: string;
  queryCount: number;
  totalCost: number;
  totalBytes: number;
  color: string;
}

// ── Component ──────────────────────────────────────────────────────

interface Props {
  connectionId: string;
  connection: AthenaConnectionSafe;
  schema: AthenaSchema | null;
}

export default function AthenaConnectionOverview({
  connectionId,
  connection,
  schema,
}: Props) {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [stats, setStats] = useState<ConnectionStats | null>(null);
  const [dailyCosts, setDailyCosts] = useState<DailyCostSummary[]>([]);
  const [sourceBreakdown, setSourceBreakdown] = useState<SourceBreakdown[]>([]);
  const [recentQueries, setRecentQueries] = useState<AthenaQueryLogEntry[]>([]);

  useEffect(() => {
    setLoading(true);
    setError(null);

    // Count schema items
    let databases = 0;
    let tables = 0;
    let columns = 0;
    if (schema?.databases) {
      databases = schema.databases.length;
      for (const db of schema.databases) {
        tables += db.tables.length;
        for (const tbl of db.tables) {
          columns += tbl.columns.length;
        }
      }
    }

    // Fetch query log
    getAthenaQueryLog(connectionId, { limit: 1000 })
      .then((log) => {
        const totalQueries = log.summary.total_queries;
        const totalCostUsd = log.summary.total_cost_usd;
        const totalBytesScanned = log.summary.total_bytes_scanned;

        const statsData: ConnectionStats = {
          databases,
          tables,
          columns,
          totalQueries,
          totalCostUsd,
          totalBytesScanned,
          avgCostPerQuery: totalQueries > 0 ? totalCostUsd / totalQueries : 0,
          avgBytesPerQuery: totalQueries > 0 ? totalBytesScanned / totalQueries : 0,
        };

        // Group by source
        const sourceMap = new Map<string, { count: number; cost: number; bytes: number }>();
        for (const entry of log.entries) {
          const src = entry.source || "unknown";
          const existing = sourceMap.get(src) || { count: 0, cost: 0, bytes: 0 };
          existing.count += 1;
          existing.cost += entry.estimated_cost_usd;
          existing.bytes += entry.data_scanned_bytes;
          sourceMap.set(src, existing);
        }

        const sourceColors: Record<string, string> = {
          user_query: "#00f0ff",
          schema_refresh_databases: "#a855f7",
          schema_refresh_tables: "#6366f1",
          schema_refresh_describe: "#8b5cf6",
          unknown: "#64748b",
        };

        const breakdown: SourceBreakdown[] = [...sourceMap.entries()].map(([src, data]) => ({
          source: src,
          queryCount: data.count,
          totalCost: data.cost,
          totalBytes: data.bytes,
          color: sourceColors[src] || "#64748b",
        })).sort((a, b) => b.totalCost - a.totalCost);

        setStats(statsData);
        setDailyCosts(log.summary.daily);
        setSourceBreakdown(breakdown);
        setRecentQueries(log.entries.slice(0, 10));
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [connectionId, schema]);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <span className="text-slate-600 text-sm font-mono animate-pulse">
          Loading overview...
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="flex items-center gap-3 px-4 py-2.5 rounded-lg"
        style={{
          background: "rgba(255, 71, 87, 0.06)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
        }}
      >
        <span className="w-2 h-2 rounded-full shrink-0 animate-pulse" style={{ background: "#ff4757" }} />
        <span className="text-xs text-red-400 font-medium">{error}</span>
      </div>
    );
  }

  if (!stats) {
    return (
      <div className="flex flex-col items-center justify-center py-20">
        <p className="text-slate-500 text-sm font-mono">No statistics available</p>
      </div>
    );
  }

  return (
    <div className="space-y-8 pb-8">
      {/* ── Connection Stats ───────────────────────────────── */}
      <div>
        <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
          Connection Statistics
        </h2>
        <div className="grid grid-cols-4 gap-3">
          <StatCard label="Databases" value={stats.databases} accent="#6366f1" />
          <StatCard label="Tables" value={stats.tables} accent="#a855f7" />
          <StatCard label="Columns" value={stats.columns} accent="#06d6a0" />
          <StatCard label="Total Queries" value={formatNumber(stats.totalQueries)} accent="#00f0ff" />
          <StatCard
            label="Total Cost"
            value={`$${stats.totalCostUsd.toFixed(4)}`}
            accent="#f59e0b"
          />
          <StatCard
            label="Data Scanned"
            value={formatBytes(stats.totalBytesScanned)}
            accent="#8b5cf6"
          />
          <StatCard
            label="Avg Cost/Query"
            value={`$${stats.avgCostPerQuery.toFixed(6)}`}
            accent="#64748b"
          />
          <StatCard
            label="Avg Bytes/Query"
            value={formatBytes(stats.avgBytesPerQuery)}
            accent="#475569"
          />
        </div>
      </div>

      {/* ── Cost Trend Chart ────────────────────────────────────── */}
      {dailyCosts.length > 1 && (
        <div>
          <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
            Daily Cost Trend
          </h2>
          <div
            className="rounded-xl p-4 relative overflow-hidden"
            style={{
              background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
              border: "1px solid rgba(16, 185, 129, 0.08)",
            }}
          >
            <div
              className="absolute top-0 left-0 w-full h-[1px]"
              style={{ background: "linear-gradient(90deg, transparent, rgba(245, 158, 11, 0.4), transparent)" }}
            />
            <div style={{ height: 220 }}>
              <CostChart data={dailyCosts} />
            </div>
          </div>
        </div>
      )}

      {/* ── Query Source Breakdown ───────────────────────────────── */}
      {sourceBreakdown.length > 0 && (
        <div>
          <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
            Queries by Source
          </h2>
          <div className="grid grid-cols-2 gap-3">
            {sourceBreakdown.map((src) => {
              const pct =
                stats.totalQueries > 0 ? (src.queryCount / stats.totalQueries) * 100 : 0;
              return (
                <div
                  key={src.source}
                  className="rounded-xl p-4 relative overflow-hidden"
                  style={{
                    background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                    border: `1px solid ${src.color}15`,
                  }}
                >
                  <div
                    className="absolute top-0 left-0 w-full h-[1px]"
                    style={{
                      background: `linear-gradient(90deg, transparent, ${src.color}40, transparent)`,
                    }}
                  />
                  <div className="flex items-center justify-between mb-2">
                    <span className="text-xs font-mono font-bold" style={{ color: src.color }}>
                      {src.source.replace(/_/g, " ")}
                    </span>
                    <span className="text-[10px] font-mono text-slate-500">
                      {pct.toFixed(1)}%
                    </span>
                  </div>
                  {/* Bar */}
                  <div
                    className="h-1.5 rounded-full overflow-hidden"
                    style={{ background: "rgba(30, 41, 59, 0.5)" }}
                  >
                    <div
                      className="h-full rounded-full transition-all"
                      style={{
                        width: `${pct}%`,
                        background: src.color,
                        opacity: 0.7,
                      }}
                    />
                  </div>
                  <div className="flex items-center justify-between mt-2">
                    <span className="text-[10px] text-slate-500 font-mono">
                      {formatNumber(src.queryCount)} queries
                    </span>
                    <span className="text-[10px] font-mono font-bold" style={{ color: "#f59e0b" }}>
                      ${src.totalCost.toFixed(4)}
                    </span>
                  </div>
                  <div className="text-[9px] text-slate-600 font-mono mt-1">
                    {formatBytes(src.totalBytes)} scanned
                  </div>
                </div>
              );
            })}
          </div>
        </div>
      )}

      {/* ── Recent Queries ───────────────────────────────── */}
      {recentQueries.length > 0 && (
        <div>
          <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
            Recent Queries
          </h2>
          <div
            className="rounded-lg overflow-hidden"
            style={{ border: "1px solid rgba(16, 185, 129, 0.08)" }}
          >
            <table className="w-full text-[10px] font-mono">
              <thead>
                <tr
                  style={{
                    background: "rgba(16, 185, 129, 0.03)",
                    borderBottom: "1px solid rgba(16, 185, 129, 0.08)",
                  }}
                >
                  <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    Time
                  </th>
                  <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    Source
                  </th>
                  <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    SQL
                  </th>
                  <th className="px-3 py-2 text-right text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    Cost
                  </th>
                  <th className="px-3 py-2 text-right text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    Bytes
                  </th>
                  <th className="px-3 py-2 text-right text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                    Duration
                  </th>
                </tr>
              </thead>
              <tbody>
                {recentQueries.map((q, i) => {
                  const isEven = i % 2 === 0;
                  const sourceColor =
                    sourceBreakdown.find((s) => s.source === q.source)?.color || "#64748b";
                  return (
                    <tr
                      key={q.entry_id}
                      className="transition-colors hover:bg-white/[0.02]"
                      style={{
                        background: isEven ? "transparent" : "rgba(0, 0, 0, 0.15)",
                        borderBottom: "1px solid rgba(16, 185, 129, 0.04)",
                      }}
                    >
                      <td className="px-3 py-1.5 text-slate-500 whitespace-nowrap">
                        {new Date(q.started_at).toLocaleTimeString()}
                      </td>
                      <td className="px-3 py-1.5">
                        <span
                          className="text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
                          style={{
                            color: sourceColor,
                            background: `${sourceColor}12`,
                            border: `1px solid ${sourceColor}25`,
                          }}
                        >
                          {q.source.replace(/schema_refresh_/, "")}
                        </span>
                      </td>
                      <td className="px-3 py-1.5 text-slate-400 truncate max-w-[300px]">
                        {q.sql.substring(0, 60)}
                        {q.sql.length > 60 ? "..." : ""}
                      </td>
                      <td className="px-3 py-1.5 text-right font-bold" style={{ color: "#f59e0b" }}>
                        ${q.estimated_cost_usd.toFixed(6)}
                      </td>
                      <td className="px-3 py-1.5 text-right text-slate-500">
                        {formatBytes(q.data_scanned_bytes)}
                      </td>
                      <td className="px-3 py-1.5 text-right text-slate-400">
                        {q.engine_execution_time_ms}ms
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        </div>
      )}
    </div>
  );
}

// ── Cost Chart (D3.js) ──────────────────────────────────────────────

function CostChart({ data }: { data: DailyCostSummary[] }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    date: string;
    cost: number;
    queries: number;
    bytes: number;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = container.clientHeight;
    const margin = { top: 12, right: 20, bottom: 32, left: 56 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    const parseDate = (s: string) => new Date(s);
    const xExtent = d3.extent(data, (d) => parseDate(d.date)) as [Date, Date];
    const yMax = d3.max(data, (d) => d.total_cost_usd) ?? 0;

    const x = d3.scaleTime().domain(xExtent).range([0, width]);
    const y = d3.scaleLinear().domain([0, yMax * 1.15]).range([height, 0]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid lines
    g.append("g")
      .call(d3.axisLeft(y).ticks(4).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // X axis
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(Math.min(data.length, 8)))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace");

    // Y axis (cost)
    g.append("g")
      .call(
        d3
          .axisLeft(y)
          .ticks(4)
          .tickFormat((d) => `$${(d as number).toFixed(3)}`),
      )
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace");

    g.selectAll(".domain, .tick line:not([stroke-dasharray])").attr("stroke", "#1e293b");

    // Area
    const area = d3
      .area<DailyCostSummary>()
      .x((d) => x(parseDate(d.date)))
      .y0(height)
      .y1((d) => y(d.total_cost_usd))
      .curve(d3.curveMonotoneX);

    g.append("path").datum(data).attr("fill", "rgba(245, 158, 11, 0.08)").attr("d", area);

    // Line
    const line = d3
      .line<DailyCostSummary>()
      .x((d) => x(parseDate(d.date)))
      .y((d) => y(d.total_cost_usd))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("fill", "none")
      .attr("stroke", "#f59e0b")
      .attr("stroke-width", 2)
      .attr("d", line);

    // Dots
    g.selectAll("circle")
      .data(data)
      .join("circle")
      .attr("cx", (d) => x(parseDate(d.date)))
      .attr("cy", (d) => y(d.total_cost_usd))
      .attr("r", 3.5)
      .attr("fill", "#f59e0b")
      .attr("opacity", 0)
      .on("mouseover", function (event, d) {
        d3.select(this).attr("opacity", 1).attr("r", 5);
        const [mx, my] = d3.pointer(event, container);
        setTooltip({
          x: mx,
          y: my,
          date: d.date,
          cost: d.total_cost_usd,
          queries: d.query_count,
          bytes: d.total_bytes_scanned,
        });
      })
      .on("mouseout", function () {
        d3.select(this).attr("opacity", 0).attr("r", 3.5);
        setTooltip(null);
      });
  }, [data]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
      {tooltip && (
        <div
          className="absolute pointer-events-none rounded-lg px-3 py-2 z-10"
          style={{
            left: tooltip.x + 12,
            top: tooltip.y - 10,
            background: "rgba(6, 8, 13, 0.92)",
            border: "1px solid rgba(245, 158, 11, 0.25)",
            boxShadow: "0 0 20px rgba(245, 158, 11, 0.08)",
            backdropFilter: "blur(8px)",
          }}
        >
          <div className="text-[10px] text-slate-400 font-mono">{tooltip.date}</div>
          <div className="text-xs font-mono font-bold mt-0.5" style={{ color: "#f59e0b" }}>
            ${tooltip.cost.toFixed(4)}
          </div>
          <div className="text-[9px] text-slate-500 font-mono mt-0.5">
            {tooltip.queries} queries &middot; {formatBytes(tooltip.bytes)}
          </div>
        </div>
      )}
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────

function StatCard({
  label,
  value,
  accent = "#10b981",
}: {
  label: string;
  value: string | number;
  accent?: string;
}) {
  return (
    <div className="stat-card rounded-xl p-4 relative overflow-hidden">
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-xl font-bold font-mono mt-1" style={{ color: accent }}>
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}

// ── Formatters ──────────────────────────────────────────────────────

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatBytes(bytes: number): string {
  if (bytes >= 1_000_000_000_000) return `${(bytes / 1_000_000_000_000).toFixed(2)} TB`;
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(2)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(1)} MB`;
  if (bytes >= 1_000) return `${(bytes / 1_000).toFixed(1)} KB`;
  return `${bytes} B`;
}
