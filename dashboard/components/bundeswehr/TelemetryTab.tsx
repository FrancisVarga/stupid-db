"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import {
  fetchAgentTelemetry,
  fetchAgentStats,
  type TelemetryEvent,
  type TelemetryStat,
} from "@/lib/api";

interface TelemetryTabProps {
  agentName: string;
}

export default function TelemetryTab({ agentName }: TelemetryTabProps) {
  const [stats, setStats] = useState<TelemetryStat | null>(null);
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      fetchAgentStats(agentName),
      fetchAgentTelemetry(agentName, 200),
    ])
      .then(([s, e]) => {
        setStats(s);
        setEvents(e);
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to fetch telemetry"))
      .finally(() => setLoading(false));
  }, [agentName]);

  if (loading) {
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-4 gap-3">
          {Array.from({ length: 4 }).map((_, i) => (
            <div
              key={i}
              className="h-20 rounded-xl animate-pulse"
              style={{ background: "rgba(0, 240, 255, 0.03)" }}
            />
          ))}
        </div>
        {Array.from({ length: 3 }).map((_, i) => (
          <div
            key={i}
            className="h-56 rounded-xl animate-pulse"
            style={{ background: "rgba(0, 240, 255, 0.02)" }}
          />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="px-4 py-3 rounded-lg text-xs"
        style={{
          background: "rgba(255, 71, 87, 0.06)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
          color: "#ff4757",
        }}
      >
        {error}
      </div>
    );
  }

  if (!stats || events.length === 0) {
    return (
      <div className="py-8 text-center">
        <div className="text-slate-500 text-sm mb-1">No telemetry data</div>
        <div className="text-slate-600 text-xs font-mono">
          Agent &quot;{agentName}&quot; has no recorded executions
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-5">
      {/* Stat cards */}
      <div className="grid grid-cols-4 gap-3">
        <StatCard label="Avg Latency" value={`${Math.round(stats.avg_latency_ms)}ms`} accent="#00f0ff" />
        <StatCard label="P95 Latency" value={`${Math.round(stats.p95_latency_ms)}ms`} accent="#a855f7" />
        <StatCard label="Total Tokens" value={formatNumber(stats.total_tokens)} accent="#06d6a0" />
        <StatCard
          label="Error Rate"
          value={`${(stats.error_rate * 100).toFixed(1)}%`}
          accent={stats.error_rate > 0.1 ? "#ff4757" : "#06d6a0"}
        />
      </div>

      {/* Latency over time */}
      <ChartPanel title="Latency Over Time">
        <LatencyChart events={events} />
      </ChartPanel>

      {/* Token usage by day */}
      <ChartPanel title="Daily Token Usage">
        <TokenBarChart events={events} />
      </ChartPanel>

      {/* Error rate by day */}
      <ChartPanel title="Daily Error Rate">
        <ErrorAreaChart events={events} />
      </ChartPanel>
    </div>
  );
}

// ── Stat Card ──────────────────────────────────────────────────

function StatCard({ label, value, accent }: { label: string; value: string; accent: string }) {
  return (
    <div className="rounded-xl p-4 relative overflow-hidden" style={{
      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
      border: `1px solid ${accent}20`,
    }}>
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-2xl font-bold font-mono mt-1" style={{ color: accent }}>{value}</div>
    </div>
  );
}

function ChartPanel({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: "1px solid rgba(0, 240, 255, 0.08)",
      }}
    >
      <div className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
        {title}
      </div>
      <div className="h-56">{children}</div>
    </div>
  );
}

// ── Latency Line Chart ─────────────────────────────────────────

function LatencyChart({ events }: { events: TelemetryEvent[] }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{ x: number; y: number; ts: string; val: number } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !events.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = container.clientHeight;
    const margin = { top: 10, right: 20, bottom: 30, left: 50 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    const sorted = [...events].sort(
      (a, b) => new Date(a.timestamp).getTime() - new Date(b.timestamp).getTime()
    );

    const x = d3
      .scaleTime()
      .domain(d3.extent(sorted, (d) => new Date(d.timestamp)) as [Date, Date])
      .range([0, width]);

    const yMax = d3.max(sorted, (d) => d.latency_ms) || 1;
    const y = d3.scaleLinear().domain([0, yMax * 1.1]).range([height, 0]);

    const g = svg.append("g").attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid
    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // Axes
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(6))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.append("g")
      .call(d3.axisLeft(y).ticks(5))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line:not([stroke-dasharray])").attr("stroke", "#1e293b");

    // Line
    const line = d3
      .line<TelemetryEvent>()
      .x((d) => x(new Date(d.timestamp)))
      .y((d) => y(d.latency_ms))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(sorted)
      .attr("fill", "none")
      .attr("stroke", "#00f0ff")
      .attr("stroke-width", 1.5)
      .attr("d", line);

    // Dots
    g.selectAll("circle")
      .data(sorted)
      .join("circle")
      .attr("cx", (d) => x(new Date(d.timestamp)))
      .attr("cy", (d) => y(d.latency_ms))
      .attr("r", 3)
      .attr("fill", "#00f0ff")
      .attr("opacity", 0)
      .on("mouseover", function (event, d) {
        d3.select(this).attr("opacity", 1).attr("r", 5);
        const [mx, my] = d3.pointer(event, container);
        setTooltip({ x: mx, y: my, ts: d.timestamp, val: d.latency_ms });
      })
      .on("mouseout", function () {
        d3.select(this).attr("opacity", 0).attr("r", 3);
        setTooltip(null);
      });
  }, [events]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
      {tooltip && <ChartTooltip x={tooltip.x} y={tooltip.y} label={new Date(tooltip.ts).toLocaleString()} value={`${tooltip.val.toLocaleString()}ms`} />}
    </div>
  );
}

// ── Token Bar Chart ────────────────────────────────────────────

function TokenBarChart({ events }: { events: TelemetryEvent[] }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{ x: number; y: number; label: string; val: number } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !events.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = container.clientHeight;
    const margin = { top: 10, right: 20, bottom: 40, left: 50 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    // Group by day
    const byDay = d3.rollup(
      events,
      (v) => d3.sum(v, (d) => d.tokens_used),
      (d) => d.timestamp.slice(0, 10)
    );
    const data = Array.from(byDay, ([day, tokens]) => ({ day, tokens })).sort(
      (a, b) => a.day.localeCompare(b.day)
    );

    const x = d3
      .scaleBand()
      .domain(data.map((d) => d.day))
      .range([0, width])
      .padding(0.3);

    const yMax = d3.max(data, (d) => d.tokens) || 1;
    const y = d3.scaleLinear().domain([0, yMax]).range([height, 0]);

    const g = svg.append("g").attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid
    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // Axes
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("transform", "rotate(-45)")
      .attr("text-anchor", "end");

    g.append("g")
      .call(d3.axisLeft(y).ticks(5))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line:not([stroke-dasharray])").attr("stroke", "#1e293b");

    // Bars
    g.selectAll("rect.bar")
      .data(data)
      .join("rect")
      .attr("class", "bar")
      .attr("x", (d) => x(d.day)!)
      .attr("width", x.bandwidth())
      .attr("y", height)
      .attr("height", 0)
      .attr("rx", 2)
      .attr("fill", "rgba(168, 85, 247, 0.5)")
      .attr("stroke", "rgba(168, 85, 247, 0.3)")
      .attr("stroke-width", 0.5)
      .on("mouseover", function (event, d) {
        d3.select(this).attr("fill", "rgba(168, 85, 247, 0.8)");
        const [mx, my] = d3.pointer(event, container);
        setTooltip({ x: mx, y: my, label: d.day, val: d.tokens });
      })
      .on("mouseout", function () {
        d3.select(this).attr("fill", "rgba(168, 85, 247, 0.5)");
        setTooltip(null);
      })
      .transition()
      .duration(600)
      .attr("y", (d) => y(d.tokens))
      .attr("height", (d) => height - y(d.tokens));
  }, [events]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
      {tooltip && <ChartTooltip x={tooltip.x} y={tooltip.y} label={tooltip.label} value={tooltip.val.toLocaleString()} />}
    </div>
  );
}

// ── Error Rate Area Chart ──────────────────────────────────────

function ErrorAreaChart({ events }: { events: TelemetryEvent[] }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{ x: number; y: number; label: string; val: string } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !events.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = container.clientHeight;
    const margin = { top: 10, right: 20, bottom: 30, left: 50 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    // Group by day: error rate %
    const byDay = d3.rollup(
      events,
      (v) => {
        const total = v.length;
        const errors = v.filter((e) => e.status === "error").length;
        return total > 0 ? (errors / total) * 100 : 0;
      },
      (d) => d.timestamp.slice(0, 10)
    );
    const data = Array.from(byDay, ([day, rate]) => ({ day, rate })).sort(
      (a, b) => a.day.localeCompare(b.day)
    );

    const parseDay = (d: string) => new Date(d);
    const x = d3
      .scaleTime()
      .domain(d3.extent(data, (d) => parseDay(d.day)) as [Date, Date])
      .range([0, width]);

    const yMax = Math.max(d3.max(data, (d) => d.rate) || 1, 5);
    const y = d3.scaleLinear().domain([0, yMax]).range([height, 0]);

    const g = svg.append("g").attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid
    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // Axes
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(6))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickFormat((d) => `${d}%`))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line:not([stroke-dasharray])").attr("stroke", "#1e293b");

    // Area
    const area = d3
      .area<{ day: string; rate: number }>()
      .x((d) => x(parseDay(d.day)))
      .y0(height)
      .y1((d) => y(d.rate))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("fill", "rgba(255, 71, 87, 0.2)")
      .attr("d", area);

    // Stroke
    const line = d3
      .line<{ day: string; rate: number }>()
      .x((d) => x(parseDay(d.day)))
      .y((d) => y(d.rate))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("fill", "none")
      .attr("stroke", "#ff4757")
      .attr("stroke-width", 1.5)
      .attr("d", line);

    // Dots
    g.selectAll("circle")
      .data(data)
      .join("circle")
      .attr("cx", (d) => x(parseDay(d.day)))
      .attr("cy", (d) => y(d.rate))
      .attr("r", 3)
      .attr("fill", "#ff4757")
      .attr("opacity", 0)
      .on("mouseover", function (event, d) {
        d3.select(this).attr("opacity", 1).attr("r", 5);
        const [mx, my] = d3.pointer(event, container);
        setTooltip({ x: mx, y: my, label: d.day, val: `${d.rate.toFixed(1)}%` });
      })
      .on("mouseout", function () {
        d3.select(this).attr("opacity", 0).attr("r", 3);
        setTooltip(null);
      });
  }, [events]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
      {tooltip && <ChartTooltip x={tooltip.x} y={tooltip.y} label={tooltip.label} value={tooltip.val} />}
    </div>
  );
}

// ── Shared Tooltip ─────────────────────────────────────────────

function ChartTooltip({ x, y, label, value }: { x: number; y: number; label: string; value: string }) {
  return (
    <div
      className="absolute pointer-events-none rounded-lg px-3 py-2 text-sm backdrop-blur-md z-10"
      style={{
        left: x + 12,
        top: y - 10,
        background: "rgba(6, 8, 13, 0.9)",
        border: "1px solid rgba(0, 240, 255, 0.2)",
        boxShadow: "0 0 20px rgba(0, 240, 255, 0.1)",
      }}
    >
      <div className="text-[10px] text-slate-400 font-mono">{label}</div>
      <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">{value}</div>
    </div>
  );
}

// ── Helpers ────────────────────────────────────────────────────

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}
