"use client";

import { useEffect, useRef, useMemo } from "react";
import * as d3 from "d3";

// ── Types ────────────────────────────────────────────────────────────

interface TopicMetrics {
  count: number;
  rate: number;
}

interface TimeSeriesPoint {
  ts: string;
  value: number;
  metric: string;
}

interface QueueMonitorPanelProps {
  topics: Record<string, TopicMetrics>;
  timeSeries: TimeSeriesPoint[];
}

// ── Thresholds ───────────────────────────────────────────────────────

const DEPTH_WARN = 100;
const DEPTH_CRIT = 500;
const RATE_WARN = 50;
const RATE_CRIT = 100;

function healthColor(value: number, warn: number, crit: number): string {
  if (value >= crit) return "#ff4757";
  if (value >= warn) return "#ffe600";
  return "#06d6a0";
}

function healthLabel(value: number, warn: number, crit: number): string {
  if (value >= crit) return "critical";
  if (value >= warn) return "elevated";
  return "healthy";
}

// ── Sparkline (D3 inline SVG) ────────────────────────────────────────

const SPARK_W = 120;
const SPARK_H = 30;
const SPARK_POINTS = 30;

function Sparkline({ data, color }: { data: number[]; color: string }) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    if (data.length < 2) {
      svg
        .append("text")
        .attr("x", SPARK_W / 2)
        .attr("y", SPARK_H / 2 + 3)
        .attr("text-anchor", "middle")
        .attr("fill", "#334155")
        .attr("font-size", 8)
        .attr("font-family", "monospace")
        .text("no data");
      return;
    }

    const x = d3
      .scaleLinear()
      .domain([0, data.length - 1])
      .range([2, SPARK_W - 2]);

    const y = d3
      .scaleLinear()
      .domain([0, d3.max(data) || 1])
      .range([SPARK_H - 2, 2]);

    const line = d3
      .line<number>()
      .x((_, i) => x(i))
      .y((d) => y(d))
      .curve(d3.curveMonotoneX);

    // Area fill under the line
    const area = d3
      .area<number>()
      .x((_, i) => x(i))
      .y0(SPARK_H)
      .y1((d) => y(d))
      .curve(d3.curveMonotoneX);

    svg
      .append("path")
      .datum(data)
      .attr("d", area)
      .attr("fill", `${color}15`);

    svg
      .append("path")
      .datum(data)
      .attr("d", line)
      .attr("fill", "none")
      .attr("stroke", color)
      .attr("stroke-width", 1.5);

    // Endpoint dot
    const lastVal = data[data.length - 1];
    svg
      .append("circle")
      .attr("cx", x(data.length - 1))
      .attr("cy", y(lastVal))
      .attr("r", 2)
      .attr("fill", color);
  }, [data, color]);

  return (
    <svg
      ref={svgRef}
      width={SPARK_W}
      height={SPARK_H}
      className="block"
      style={{ overflow: "visible" }}
    />
  );
}

// ── Depth Bar (horizontal D3 bar) ────────────────────────────────────

const BAR_W = 200;
const BAR_H = 14;

function DepthBar({ value, maxValue }: { value: number; maxValue: number }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const color = healthColor(value, DEPTH_WARN, DEPTH_CRIT);

  useEffect(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const scale = d3
      .scaleLinear()
      .domain([0, maxValue || 1])
      .range([0, BAR_W - 4])
      .clamp(true);

    // Background track
    svg
      .append("rect")
      .attr("x", 0)
      .attr("y", 2)
      .attr("width", BAR_W)
      .attr("height", BAR_H - 4)
      .attr("rx", 3)
      .attr("fill", `${color}15`);

    // Filled bar
    svg
      .append("rect")
      .attr("x", 2)
      .attr("y", 3)
      .attr("width", Math.max(scale(value), 2))
      .attr("height", BAR_H - 6)
      .attr("rx", 2)
      .attr("fill", color)
      .attr("opacity", 0.8);

    // Threshold markers
    [DEPTH_WARN, DEPTH_CRIT].forEach((thresh) => {
      if (thresh <= maxValue) {
        svg
          .append("line")
          .attr("x1", scale(thresh) + 2)
          .attr("x2", scale(thresh) + 2)
          .attr("y1", 1)
          .attr("y2", BAR_H - 1)
          .attr("stroke", healthColor(thresh, DEPTH_WARN, DEPTH_CRIT))
          .attr("stroke-width", 1)
          .attr("stroke-dasharray", "2,2")
          .attr("opacity", 0.4);
      }
    });
  }, [value, maxValue, color]);

  return (
    <svg
      ref={svgRef}
      width={BAR_W}
      height={BAR_H}
      className="block"
      style={{ overflow: "visible" }}
    />
  );
}

// ── Main Panel ───────────────────────────────────────────────────────

export default function QueueMonitorPanel({
  topics,
  timeSeries,
}: QueueMonitorPanelProps) {
  const topicEntries = useMemo(
    () =>
      Object.entries(topics).sort(
        ([, a], [, b]) => b.count - a.count,
      ),
    [topics],
  );

  // Build sparkline data per topic from time_series
  const sparklineData = useMemo(() => {
    const map: Record<string, number[]> = {};
    for (const [name] of topicEntries) {
      // Match time_series metric names like "topic.<name>.rate" or just the topic name
      const points = timeSeries
        .filter(
          (p) =>
            p.metric === name ||
            p.metric === `topic.${name}.rate` ||
            p.metric === `${name}.rate`,
        )
        .map((p) => p.value)
        .slice(-SPARK_POINTS);
      map[name] = points;
    }
    return map;
  }, [topicEntries, timeSeries]);

  const maxDepth = useMemo(
    () => Math.max(...topicEntries.map(([, t]) => t.count), DEPTH_CRIT, 1),
    [topicEntries],
  );

  if (topicEntries.length === 0) return null;

  return (
    <div
      className="rounded-xl overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: "1px solid rgba(244, 114, 182, 0.1)",
        boxShadow: "0 0 30px rgba(244, 114, 182, 0.03)",
      }}
    >
      {/* Header row */}
      <div
        className="grid items-center px-4 py-3"
        style={{
          gridTemplateColumns: "1.2fr 200px 120px 100px 80px",
          borderBottom: "1px solid rgba(244, 114, 182, 0.08)",
        }}
      >
        {["Topic", "Queue Depth", "Throughput", "Consumer Lag", "Status"].map(
          (h) => (
            <div
              key={h}
              className={`text-[10px] text-slate-500 uppercase tracking-widest font-medium ${
                h === "Topic" ? "text-left" : "text-center"
              }`}
            >
              {h}
            </div>
          ),
        )}
      </div>

      {/* Topic rows */}
      {topicEntries.map(([name, tm]) => {
        const depthColor = healthColor(tm.count, DEPTH_WARN, DEPTH_CRIT);
        const rateColor = healthColor(tm.rate, RATE_WARN, RATE_CRIT);
        const status = healthLabel(
          Math.max(tm.count / DEPTH_CRIT, tm.rate / RATE_CRIT),
          DEPTH_WARN / DEPTH_CRIT,
          1,
        );
        // Consumer lag: approximate from rate vs depth trend
        // Positive depth + rate means messages accumulating faster than consuming
        const lagEstimate = tm.count > 0 ? tm.count / Math.max(tm.rate, 0.1) : 0;
        const isBackpressure =
          tm.count >= DEPTH_CRIT || tm.rate >= RATE_CRIT;

        return (
          <div
            key={name}
            className="grid items-center px-4 py-3 transition-colors hover:bg-white/[0.02]"
            style={{
              gridTemplateColumns: "1.2fr 200px 120px 100px 80px",
              borderBottom: "1px solid rgba(30, 41, 59, 0.4)",
            }}
          >
            {/* Topic name */}
            <div className="flex items-center gap-2">
              <span className="text-sm font-mono text-slate-300 truncate">
                {name}
              </span>
              {isBackpressure && (
                <span
                  className="text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded animate-pulse"
                  style={{
                    color: "#ff4757",
                    background: "rgba(255, 71, 87, 0.15)",
                    border: "1px solid rgba(255, 71, 87, 0.3)",
                  }}
                >
                  Backpressure
                </span>
              )}
            </div>

            {/* Depth bar */}
            <div className="flex items-center gap-2 justify-center">
              <DepthBar value={tm.count} maxValue={maxDepth} />
              <span
                className="text-xs font-mono font-bold min-w-[3rem] text-right"
                style={{ color: depthColor }}
              >
                {tm.count.toLocaleString()}
              </span>
            </div>

            {/* Sparkline + rate */}
            <div className="flex flex-col items-center gap-0.5">
              <Sparkline
                data={sparklineData[name] || []}
                color={rateColor}
              />
              <span
                className="text-[10px] font-mono font-bold"
                style={{ color: rateColor }}
              >
                {tm.rate.toFixed(1)} msg/s
              </span>
            </div>

            {/* Consumer lag */}
            <div className="text-center">
              <span
                className="text-sm font-mono font-bold"
                style={{
                  color:
                    lagEstimate > 10
                      ? "#ff4757"
                      : lagEstimate > 3
                        ? "#ffe600"
                        : "#06d6a0",
                }}
              >
                {lagEstimate < 0.1
                  ? "0s"
                  : lagEstimate < 60
                    ? `${lagEstimate.toFixed(1)}s`
                    : `${(lagEstimate / 60).toFixed(1)}m`}
              </span>
              <div className="text-[9px] text-slate-600 font-mono">
                est. drain
              </div>
            </div>

            {/* Status badge */}
            <div className="flex justify-center">
              <span
                className="text-[9px] font-bold uppercase tracking-wider px-2 py-0.5 rounded-full"
                style={{
                  color:
                    status === "critical"
                      ? "#ff4757"
                      : status === "elevated"
                        ? "#ffe600"
                        : "#06d6a0",
                  background:
                    status === "critical"
                      ? "rgba(255, 71, 87, 0.1)"
                      : status === "elevated"
                        ? "rgba(255, 230, 0, 0.1)"
                        : "rgba(6, 214, 160, 0.08)",
                  border: `1px solid ${
                    status === "critical"
                      ? "rgba(255, 71, 87, 0.2)"
                      : status === "elevated"
                        ? "rgba(255, 230, 0, 0.2)"
                        : "rgba(6, 214, 160, 0.15)"
                  }`,
                }}
              >
                {status}
              </span>
            </div>
          </div>
        );
      })}
    </div>
  );
}
