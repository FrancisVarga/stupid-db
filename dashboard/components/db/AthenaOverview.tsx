"use client";

import { useEffect, useState, useRef } from "react";
import Link from "next/link";
import * as d3 from "d3";
import {
  listAthenaConnections,
  getAthenaQueryLog,
  type AthenaConnectionSafe,
  type QueryLogResponse,
  type DailyCostSummary,
} from "@/lib/db/athena-connections";

// ── Types ──────────────────────────────────────────────────────────

interface ConnectionStats {
  connection: AthenaConnectionSafe;
  databases: number;
  tables: number;
  columns: number;
  totalQueries: number;
  totalCostUsd: number;
  totalBytesScanned: number;
}

interface AggregatedStats {
  connections: number;
  databases: number;
  tables: number;
  columns: number;
  totalQueries: number;
  totalCostUsd: number;
  totalBytesScanned: number;
  readyCount: number;
}

// ── Component ──────────────────────────────────────────────────────

export default function AthenaOverview() {
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [connStats, setConnStats] = useState<ConnectionStats[]>([]);
  const [aggregated, setAggregated] = useState<AggregatedStats | null>(null);
  const [dailyCosts, setDailyCosts] = useState<DailyCostSummary[]>([]);
  const [sortCol, setSortCol] = useState<
    "name" | "databases" | "tables" | "columns" | "totalQueries" | "totalCostUsd"
  >("totalCostUsd");
  const [sortAsc, setSortAsc] = useState(false);

  useEffect(() => {
    setLoading(true);
    setError(null);

    listAthenaConnections()
      .then(async (connections) => {
        // Fetch query logs for all connections in parallel
        const logResults = await Promise.allSettled(
          connections.map((c) => getAthenaQueryLog(c.id, { limit: 1000 })),
        );

        const allDailyCosts: DailyCostSummary[] = [];
        const stats: ConnectionStats[] = connections.map((conn, i) => {
          const logResult = logResults[i];
          const log: QueryLogResponse | null =
            logResult.status === "fulfilled" ? logResult.value : null;

          // Count schema items
          let databases = 0;
          let tables = 0;
          let columns = 0;
          if (conn.schema?.databases) {
            databases = conn.schema.databases.length;
            for (const db of conn.schema.databases) {
              tables += db.tables.length;
              for (const tbl of db.tables) {
                columns += tbl.columns.length;
              }
            }
          }

          // Merge daily costs
          if (log?.summary?.daily) {
            for (const day of log.summary.daily) {
              allDailyCosts.push(day);
            }
          }

          return {
            connection: conn,
            databases,
            tables,
            columns,
            totalQueries: log?.summary?.total_queries ?? 0,
            totalCostUsd: log?.summary?.total_cost_usd ?? 0,
            totalBytesScanned: log?.summary?.total_bytes_scanned ?? 0,
          };
        });

        // Aggregate daily costs by date across connections
        const costByDate = new Map<string, DailyCostSummary>();
        for (const d of allDailyCosts) {
          const existing = costByDate.get(d.date);
          if (existing) {
            existing.query_count += d.query_count;
            existing.total_bytes_scanned += d.total_bytes_scanned;
            existing.total_cost_usd += d.total_cost_usd;
          } else {
            costByDate.set(d.date, { ...d });
          }
        }
        const mergedDaily = [...costByDate.values()].sort(
          (a, b) => a.date.localeCompare(b.date),
        );

        const agg: AggregatedStats = {
          connections: connections.length,
          databases: stats.reduce((s, c) => s + c.databases, 0),
          tables: stats.reduce((s, c) => s + c.tables, 0),
          columns: stats.reduce((s, c) => s + c.columns, 0),
          totalQueries: stats.reduce((s, c) => s + c.totalQueries, 0),
          totalCostUsd: stats.reduce((s, c) => s + c.totalCostUsd, 0),
          totalBytesScanned: stats.reduce((s, c) => s + c.totalBytesScanned, 0),
          readyCount: connections.filter((c) => c.schema_status === "ready").length,
        };

        setConnStats(stats);
        setAggregated(agg);
        setDailyCosts(mergedDaily);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  const handleSort = (
    col: typeof sortCol,
  ) => {
    if (sortCol === col) {
      setSortAsc((prev) => !prev);
    } else {
      setSortCol(col);
      setSortAsc(col === "name");
    }
  };

  const sorted = [...connStats].sort((a, b) => {
    const dir = sortAsc ? 1 : -1;
    if (sortCol === "name") return a.connection.name.localeCompare(b.connection.name) * dir;
    return (a[sortCol] - b[sortCol]) * dir;
  });

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
        className="flex items-center gap-3 px-4 py-2.5 rounded-lg mx-4 mt-4"
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

  if (!aggregated || connStats.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-20">
        <p className="text-slate-500 text-sm font-mono mb-2">No Athena connections configured</p>
        <p className="text-slate-600 text-xs font-mono">Add a connection to see overview statistics</p>
      </div>
    );
  }

  return (
    <div className="space-y-8">
      {/* ── Aggregated Stat Cards ───────────────────────────────── */}
      <div>
        <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
          Overall Statistics
        </h2>
        <div className="grid grid-cols-4 gap-3">
          <StatCard
            label="Connections"
            value={`${aggregated.readyCount}/${aggregated.connections}`}
            sub="ready"
            accent="#10b981"
          />
          <StatCard label="Databases" value={aggregated.databases} accent="#6366f1" />
          <StatCard label="Tables" value={aggregated.tables} accent="#a855f7" />
          <StatCard label="Columns" value={aggregated.columns} accent="#06d6a0" />
          <StatCard
            label="Total Queries"
            value={formatNumber(aggregated.totalQueries)}
            accent="#00f0ff"
          />
          <StatCard
            label="Total Cost"
            value={`$${aggregated.totalCostUsd.toFixed(4)}`}
            accent="#f59e0b"
          />
          <StatCard
            label="Data Scanned"
            value={formatBytes(aggregated.totalBytesScanned)}
            accent="#8b5cf6"
          />
          <StatCard
            label="Avg Cost/Query"
            value={
              aggregated.totalQueries > 0
                ? `$${(aggregated.totalCostUsd / aggregated.totalQueries).toFixed(6)}`
                : "$0"
            }
            accent="#64748b"
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

      {/* ── Per-Connection Breakdown ────────────────────────────── */}
      <div>
        <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
          Per-Connection Breakdown
        </h2>
        <div
          className="rounded-lg overflow-hidden"
          style={{ border: "1px solid rgba(16, 185, 129, 0.08)" }}
        >
          <table className="w-full text-[11px] font-mono">
            <thead>
              <tr
                style={{
                  background: "rgba(16, 185, 129, 0.03)",
                  borderBottom: "1px solid rgba(16, 185, 129, 0.08)",
                }}
              >
                <SortHeader label="Connection" col="name" active={sortCol} asc={sortAsc} onClick={handleSort} />
                <th className="px-4 py-2.5 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                  Region
                </th>
                <th className="px-4 py-2.5 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                  Status
                </th>
                <SortHeader label="DBs" col="databases" active={sortCol} asc={sortAsc} onClick={handleSort} align="right" />
                <SortHeader label="Tables" col="tables" active={sortCol} asc={sortAsc} onClick={handleSort} align="right" />
                <SortHeader label="Columns" col="columns" active={sortCol} asc={sortAsc} onClick={handleSort} align="right" />
                <SortHeader label="Queries" col="totalQueries" active={sortCol} asc={sortAsc} onClick={handleSort} align="right" />
                <SortHeader label="Cost (USD)" col="totalCostUsd" active={sortCol} asc={sortAsc} onClick={handleSort} align="right" />
              </tr>
            </thead>
            <tbody>
              {sorted.map((cs, i) => {
                const isEven = i % 2 === 0;
                const conn = cs.connection;
                return (
                  <tr
                    key={conn.id}
                    className="transition-colors hover:bg-white/[0.02]"
                    style={{
                      background: isEven ? "transparent" : "rgba(0, 0, 0, 0.15)",
                      borderBottom: "1px solid rgba(16, 185, 129, 0.04)",
                    }}
                  >
                    <td className="px-4 py-2">
                      <Link
                        href={`/athena/${encodeURIComponent(conn.id)}`}
                        className="font-semibold transition-colors hover:underline"
                        style={{ color: conn.color }}
                      >
                        {conn.name}
                      </Link>
                    </td>
                    <td className="px-4 py-2 text-slate-500">{conn.region}</td>
                    <td className="px-4 py-2">
                      <SchemaStatusBadge status={conn.schema_status} />
                    </td>
                    <td className="px-4 py-2 text-right text-slate-400">{cs.databases}</td>
                    <td className="px-4 py-2 text-right text-slate-400">{cs.tables}</td>
                    <td className="px-4 py-2 text-right text-slate-500">{cs.columns}</td>
                    <td className="px-4 py-2 text-right text-slate-400">
                      {formatNumber(cs.totalQueries)}
                    </td>
                    <td className="px-4 py-2 text-right font-bold" style={{ color: "#f59e0b" }}>
                      ${cs.totalCostUsd.toFixed(4)}
                    </td>
                  </tr>
                );
              })}
            </tbody>
            {/* Totals footer */}
            <tfoot>
              <tr
                style={{
                  borderTop: "1px solid rgba(16, 185, 129, 0.12)",
                  background: "rgba(16, 185, 129, 0.02)",
                }}
              >
                <td className="px-4 py-2 text-[10px] font-bold uppercase tracking-wider text-slate-500" colSpan={3}>
                  Total
                </td>
                <td className="px-4 py-2 text-right font-bold text-slate-300">
                  {aggregated.databases}
                </td>
                <td className="px-4 py-2 text-right font-bold text-slate-300">
                  {aggregated.tables}
                </td>
                <td className="px-4 py-2 text-right font-bold text-slate-300">
                  {aggregated.columns}
                </td>
                <td className="px-4 py-2 text-right font-bold text-slate-300">
                  {formatNumber(aggregated.totalQueries)}
                </td>
                <td className="px-4 py-2 text-right font-bold" style={{ color: "#f59e0b" }}>
                  ${aggregated.totalCostUsd.toFixed(4)}
                </td>
              </tr>
            </tfoot>
          </table>
        </div>
      </div>

      {/* ── Data Volume Breakdown ───────────────────────────────── */}
      {connStats.some((cs) => cs.totalBytesScanned > 0) && (
        <div>
          <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
            Data Volume by Connection
          </h2>
          <div className="grid grid-cols-2 gap-3">
            {connStats
              .filter((cs) => cs.totalBytesScanned > 0)
              .sort((a, b) => b.totalBytesScanned - a.totalBytesScanned)
              .map((cs) => {
                const pct =
                  aggregated.totalBytesScanned > 0
                    ? (cs.totalBytesScanned / aggregated.totalBytesScanned) * 100
                    : 0;
                return (
                  <div
                    key={cs.connection.id}
                    className="rounded-xl p-4 relative overflow-hidden"
                    style={{
                      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                      border: `1px solid ${cs.connection.color}15`,
                    }}
                  >
                    <div
                      className="absolute top-0 left-0 w-full h-[1px]"
                      style={{
                        background: `linear-gradient(90deg, transparent, ${cs.connection.color}40, transparent)`,
                      }}
                    />
                    <div className="flex items-center justify-between mb-2">
                      <span className="text-xs font-mono font-bold" style={{ color: cs.connection.color }}>
                        {cs.connection.name}
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
                          background: cs.connection.color,
                          opacity: 0.7,
                        }}
                      />
                    </div>
                    <div className="flex items-center justify-between mt-2">
                      <span className="text-[10px] text-slate-500 font-mono">
                        {formatBytes(cs.totalBytesScanned)}
                      </span>
                      <span className="text-[10px] text-slate-600 font-mono">
                        {formatNumber(cs.totalQueries)} queries
                      </span>
                    </div>
                  </div>
                );
              })}
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
  sub,
  accent = "#10b981",
}: {
  label: string;
  value: string | number;
  sub?: string;
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
      {sub && (
        <div className="text-[9px] text-slate-600 font-mono mt-0.5">{sub}</div>
      )}
    </div>
  );
}

function SortHeader({
  label,
  col,
  active,
  asc,
  onClick,
  align = "left",
}: {
  label: string;
  col: string;
  active: string;
  asc: boolean;
  onClick: (col: "name" | "databases" | "tables" | "columns" | "totalQueries" | "totalCostUsd") => void;
  align?: "left" | "right";
}) {
  const isActive = active === col;
  return (
    <th
      className={`px-4 py-2.5 text-${align} text-[9px] font-bold tracking-wider uppercase cursor-pointer select-none transition-colors hover:text-slate-300`}
      style={{ color: isActive ? "#10b981" : "#64748b" }}
      onClick={() => onClick(col as "name" | "databases" | "tables" | "columns" | "totalQueries" | "totalCostUsd")}
    >
      {label}
      {isActive && (
        <span className="ml-1 text-[8px]">{asc ? "\u25B2" : "\u25BC"}</span>
      )}
    </th>
  );
}

function SchemaStatusBadge({ status }: { status: string }) {
  const normalized = status.startsWith("failed") ? "failed" : status;
  let bg: string;
  let badgeColor: string;
  let pulse = false;

  switch (normalized) {
    case "ready":
      bg = "rgba(6, 214, 160, 0.1)";
      badgeColor = "#06d6a0";
      break;
    case "pending":
      bg = "rgba(250, 204, 21, 0.1)";
      badgeColor = "#facc15";
      pulse = true;
      break;
    case "fetching":
      bg = "rgba(59, 130, 246, 0.1)";
      badgeColor = "#3b82f6";
      pulse = true;
      break;
    case "error":
    case "failed":
      bg = "rgba(255, 71, 87, 0.1)";
      badgeColor = "#ff4757";
      break;
    default:
      bg = "rgba(100, 116, 139, 0.1)";
      badgeColor = "#64748b";
  }

  return (
    <span
      className={`text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded${pulse ? " animate-pulse" : ""}`}
      style={{ background: bg, color: badgeColor }}
    >
      {normalized}
    </span>
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
