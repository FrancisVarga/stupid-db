"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import * as d3 from "d3";

// ── Types ──────────────────────────────────────────────────────────────

interface StageLatency {
  ts: Date;
  ingest: number;
  compute: number;
  graph: number;
}

interface Percentiles {
  p50: number;
  p95: number;
  p99: number;
}

interface StagePercentiles {
  ingest: Percentiles;
  compute: Percentiles;
  graph: Percentiles;
}

interface TooltipData {
  x: number;
  y: number;
  ts: Date;
  ingest: number;
  compute: number;
  graph: number;
  total: number;
  anomaly: boolean;
}

interface Props {
  refreshKey: number;
}

// ── Constants ──────────────────────────────────────────────────────────

const STAGES = ["ingest", "compute", "graph"] as const;
type Stage = (typeof STAGES)[number];

const STAGE_COLORS: Record<Stage, string> = {
  ingest: "#3b82f6",   // blue
  compute: "#f97316",  // orange
  graph: "#06d6a0",    // green
};

const WINDOW_SECS = 300; // 5 minutes
const MAX_POINTS = 60;   // ~one per 5s

// ── Helpers ────────────────────────────────────────────────────────────

function percentile(sorted: number[], p: number): number {
  if (sorted.length === 0) return 0;
  const idx = (p / 100) * (sorted.length - 1);
  const lo = Math.floor(idx);
  const hi = Math.ceil(idx);
  if (lo === hi) return sorted[lo];
  return sorted[lo] + (sorted[hi] - sorted[lo]) * (idx - lo);
}

function computePercentiles(values: number[]): Percentiles {
  const sorted = [...values].sort((a, b) => a - b);
  return {
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    p99: percentile(sorted, 99),
  };
}

function generateLatencyPoint(): StageLatency {
  // Simulated latency: ingest 5-25ms, compute 10-40ms, graph 3-15ms
  // Occasional spikes
  const spike = Math.random() < 0.08;
  const multiplier = spike ? 3 + Math.random() * 4 : 1;
  return {
    ts: new Date(),
    ingest: (5 + Math.random() * 20) * (spike && Math.random() > 0.5 ? multiplier : 1),
    compute: (10 + Math.random() * 30) * (spike ? multiplier : 1),
    graph: (3 + Math.random() * 12) * (spike && Math.random() > 0.3 ? multiplier : 1),
  };
}

// ── Component ──────────────────────────────────────────────────────────

export default function PipelineLatencyChart({ refreshKey }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const historyRef = useRef<StageLatency[]>([]);
  const [tooltip, setTooltip] = useState<TooltipData | null>(null);
  const [percentiles, setPercentiles] = useState<StagePercentiles | null>(null);

  // Accumulate data points on each refresh
  const addPoint = useCallback(() => {
    const point = generateLatencyPoint();
    const cutoff = Date.now() - WINDOW_SECS * 1000;
    historyRef.current = [
      ...historyRef.current.filter((p) => p.ts.getTime() > cutoff),
      point,
    ].slice(-MAX_POINTS);
  }, []);

  // Compute percentiles from history
  const updatePercentiles = useCallback(() => {
    const h = historyRef.current;
    if (h.length < 3) return;
    setPercentiles({
      ingest: computePercentiles(h.map((d) => d.ingest)),
      compute: computePercentiles(h.map((d) => d.compute)),
      graph: computePercentiles(h.map((d) => d.graph)),
    });
  }, []);

  // Add point on each refresh tick
  useEffect(() => {
    addPoint();
    updatePercentiles();
  }, [refreshKey, addPoint, updatePercentiles]);

  // D3 rendering
  useEffect(() => {
    if (!svgRef.current || !containerRef.current) return;
    const data = historyRef.current;
    if (data.length < 2) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = 320;
    const margin = { top: 12, right: 16, bottom: 32, left: 48 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Scales
    const xExtent = d3.extent(data, (d) => d.ts) as [Date, Date];
    const x = d3.scaleTime().domain(xExtent).range([0, width]);

    const maxTotal = d3.max(data, (d) => d.ingest + d.compute + d.graph) || 100;
    const y = d3.scaleLinear().domain([0, maxTotal * 1.15]).range([height, 0]);

    // Anomaly threshold: 2x p95 total
    const p95Total = percentiles
      ? percentiles.ingest.p95 + percentiles.compute.p95 + percentiles.graph.p95
      : Infinity;
    const anomalyThreshold = p95Total * 2;

    // Grid lines
    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // Anomaly threshold line
    if (percentiles && anomalyThreshold <= maxTotal * 1.15) {
      g.append("line")
        .attr("x1", 0)
        .attr("x2", width)
        .attr("y1", y(anomalyThreshold))
        .attr("y2", y(anomalyThreshold))
        .attr("stroke", "#ff475780")
        .attr("stroke-width", 1)
        .attr("stroke-dasharray", "4,3");

      g.append("text")
        .attr("x", width - 4)
        .attr("y", y(anomalyThreshold) - 4)
        .attr("text-anchor", "end")
        .attr("fill", "#ff4757")
        .attr("font-size", "8px")
        .attr("font-family", "monospace")
        .text("2× p95");
    }

    // Stacked bar width based on data density
    const barWidth = Math.max(2, Math.min(12, (width / data.length) * 0.7));

    // Stack generator
    const stack = d3
      .stack<StageLatency>()
      .keys(STAGES as unknown as string[])
      .order(d3.stackOrderNone)
      .offset(d3.stackOffsetNone);

    const stacked = stack(data);

    // Draw stacked bars
    for (const layer of stacked) {
      const stage = layer.key as Stage;
      const color = STAGE_COLORS[stage];

      g.selectAll(`rect.bar-${stage}`)
        .data(layer)
        .join("rect")
        .attr("class", `bar-${stage}`)
        .attr("x", (d) => x(d.data.ts) - barWidth / 2)
        .attr("y", (d) => y(d[1]))
        .attr("height", (d) => Math.max(0, y(d[0]) - y(d[1])))
        .attr("width", barWidth)
        .attr("rx", 1)
        .attr("fill", color + "70")
        .attr("stroke", color + "40")
        .attr("stroke-width", 0.5);
    }

    // Anomaly highlights (red outline on bars exceeding threshold)
    data.forEach((d) => {
      const total = d.ingest + d.compute + d.graph;
      if (total > anomalyThreshold) {
        g.append("rect")
          .attr("x", x(d.ts) - barWidth / 2 - 1)
          .attr("y", y(total) - 1)
          .attr("width", barWidth + 2)
          .attr("height", Math.max(0, height - y(total) + 2))
          .attr("rx", 2)
          .attr("fill", "none")
          .attr("stroke", "#ff4757")
          .attr("stroke-width", 1.5)
          .attr("opacity", 0.8);
      }
    });

    // Total latency line overlay
    const totalLine = d3
      .line<StageLatency>()
      .x((d) => x(d.ts))
      .y((d) => y(d.ingest + d.compute + d.graph))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("fill", "none")
      .attr("stroke", "#f472b6")
      .attr("stroke-width", 1.5)
      .attr("opacity", 0.6)
      .attr("d", totalLine);

    // Invisible hover rects for tooltips
    data.forEach((d) => {
      const total = d.ingest + d.compute + d.graph;
      const isAnomaly = total > anomalyThreshold;

      g.append("rect")
        .attr("x", x(d.ts) - barWidth)
        .attr("y", 0)
        .attr("width", barWidth * 2)
        .attr("height", height)
        .attr("fill", "transparent")
        .attr("cursor", "crosshair")
        .on("mouseover", function (event) {
          const [mx, my] = d3.pointer(event, container);
          setTooltip({
            x: mx,
            y: my,
            ts: d.ts,
            ingest: d.ingest,
            compute: d.compute,
            graph: d.graph,
            total,
            anomaly: isAnomaly,
          });
        })
        .on("mouseout", () => setTooltip(null));
    });

    // X axis
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(
        d3
          .axisBottom(x)
          .ticks(6)
          .tickFormat((d) => d3.timeFormat("%H:%M:%S")(d as Date))
      )
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    // Y axis
    g.append("g")
      .call(
        d3
          .axisLeft(y)
          .ticks(5)
          .tickFormat((d) => `${d}ms`)
      )
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line").attr("stroke", "#1e293b");
  }, [refreshKey, percentiles]);

  return (
    <div className="space-y-3">
      {/* Chart */}
      <div ref={containerRef} className="relative w-full overflow-hidden">
        <svg ref={svgRef} className="w-full" />
        {tooltip && (
          <div
            className="absolute pointer-events-none rounded-lg px-3 py-2.5 text-sm backdrop-blur-md z-10"
            style={{
              left: Math.min(tooltip.x + 12, (containerRef.current?.clientWidth || 400) - 180),
              top: tooltip.y - 10,
              background: "rgba(6, 8, 13, 0.95)",
              border: `1px solid ${tooltip.anomaly ? "rgba(255, 71, 87, 0.4)" : "rgba(244, 114, 182, 0.2)"}`,
              boxShadow: tooltip.anomaly
                ? "0 0 20px rgba(255, 71, 87, 0.15)"
                : "0 0 20px rgba(244, 114, 182, 0.1)",
            }}
          >
            <div className="text-[10px] text-slate-500 font-mono mb-1.5">
              {tooltip.ts.toLocaleTimeString()}
              {tooltip.anomaly && (
                <span className="ml-2 text-red-400 font-bold">SPIKE</span>
              )}
            </div>
            {STAGES.map((stage) => (
              <div key={stage} className="flex items-center gap-2 text-[11px] font-mono">
                <div
                  className="w-2 h-2 rounded-sm"
                  style={{ background: STAGE_COLORS[stage] }}
                />
                <span className="text-slate-400 w-14">{stage}</span>
                <span className="text-slate-200 font-bold">
                  {tooltip[stage].toFixed(1)}ms
                </span>
              </div>
            ))}
            <div className="mt-1 pt-1 text-[11px] font-mono font-bold"
              style={{
                borderTop: "1px solid rgba(100, 116, 139, 0.2)",
                color: tooltip.anomaly ? "#ff4757" : "#f472b6",
              }}
            >
              Total: {tooltip.total.toFixed(1)}ms
            </div>
          </div>
        )}
      </div>

      {/* Legend + Percentiles */}
      <div className="flex flex-wrap items-start gap-4 px-1">
        {/* Stage legend */}
        <div className="flex items-center gap-3">
          {STAGES.map((stage) => (
            <div key={stage} className="flex items-center gap-1.5">
              <div
                className="w-2.5 h-2.5 rounded-sm"
                style={{ background: STAGE_COLORS[stage] }}
              />
              <span className="text-[10px] text-slate-500 font-mono uppercase tracking-wider">
                {stage}
              </span>
            </div>
          ))}
          <div className="flex items-center gap-1.5 ml-1">
            <div className="w-4 h-[2px] rounded" style={{ background: "#f472b6" }} />
            <span className="text-[10px] text-slate-500 font-mono uppercase tracking-wider">
              total
            </span>
          </div>
        </div>

        {/* Percentile table */}
        {percentiles && (
          <div className="flex gap-3 ml-auto">
            {STAGES.map((stage) => {
              const p = percentiles[stage];
              return (
                <div
                  key={stage}
                  className="rounded-lg px-2.5 py-1.5"
                  style={{
                    background: "rgba(15, 23, 42, 0.6)",
                    border: `1px solid ${STAGE_COLORS[stage]}20`,
                  }}
                >
                  <div
                    className="text-[9px] font-bold uppercase tracking-wider mb-1"
                    style={{ color: STAGE_COLORS[stage] }}
                  >
                    {stage}
                  </div>
                  <div className="flex gap-2">
                    {(["p50", "p95", "p99"] as const).map((pKey) => (
                      <div key={pKey} className="text-center">
                        <div className="text-[8px] text-slate-600 font-mono uppercase">
                          {pKey}
                        </div>
                        <div className="text-[10px] text-slate-300 font-mono font-bold">
                          {p[pKey].toFixed(0)}
                        </div>
                      </div>
                    ))}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
