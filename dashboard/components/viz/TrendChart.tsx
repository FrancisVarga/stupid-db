"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { TrendEntry } from "@/lib/api";

const DIRECTION_ARROWS: Record<string, string> = {
  Up: "\u2191",
  Down: "\u2193",
  Stable: "\u2192",
};

function magnitudeColor(mag: number): string {
  if (mag >= 3.0) return "#ff4757"; // Critical
  if (mag >= 2.0) return "#ffe600"; // Significant
  return "#06d6a0"; // Notable
}

function severityLabel(mag: number): string {
  if (mag >= 3.0) return "Critical";
  if (mag >= 2.0) return "Significant";
  return "Notable";
}

function directionColor(dir: string): string {
  if (dir === "Up") return "#06d6a0";
  if (dir === "Down") return "#ff4757";
  return "#64748b";
}

interface Props {
  data: TrendEntry[];
}

export default function TrendChart({ data }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [selectedMetric, setSelectedMetric] = useState<string | null>(null);

  const handleRowClick = useCallback(
    (_event: MouseEvent, d: TrendEntry) => {
      setSelectedMetric((prev) => (prev === d.metric ? null : d.metric));
    },
    []
  );

  useEffect(() => {
    if (!svgRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = svgRef.current.parentElement!;
    const width = container.clientWidth;
    const rowHeight = 36;
    const margin = { top: 24, right: 20, bottom: 8, left: 140 };
    const height = Math.max(
      data.length * rowHeight + margin.top + margin.bottom,
      200
    );

    svg.attr("width", width).attr("height", height);

    const maxMag = d3.max(data, (d) => Math.abs(d.magnitude)) || 1;
    const barWidth = width - margin.left - margin.right;

    const x = d3
      .scaleLinear()
      .domain([0, maxMag])
      .range([0, barWidth * 0.6]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Header
    svg
      .append("text")
      .attr("x", margin.left)
      .attr("y", 14)
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .attr("letter-spacing", "0.1em")
      .text("METRIC");
    svg
      .append("text")
      .attr("x", margin.left + barWidth * 0.65)
      .attr("y", 14)
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .attr("letter-spacing", "0.1em")
      .text("MAGNITUDE");
    svg
      .append("text")
      .attr("x", width - margin.right)
      .attr("y", 14)
      .attr("text-anchor", "end")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .attr("letter-spacing", "0.1em")
      .text("VALUE");

    const rows = g
      .selectAll("g.row")
      .data(data)
      .join("g")
      .attr("class", "row")
      .attr("transform", (_, i) => `translate(0,${i * rowHeight})`)
      .style("cursor", "pointer")
      .on("click", handleRowClick as never);

    // Invisible hit area for easier clicking
    rows
      .append("rect")
      .attr("x", -margin.left)
      .attr("y", 0)
      .attr("width", width)
      .attr("height", rowHeight)
      .attr("fill", "transparent");

    // Hover highlight
    rows
      .on("mouseenter", function () {
        d3.select(this)
          .select("rect")
          .attr("fill", "rgba(0, 240, 255, 0.04)");
      })
      .on("mouseleave", function () {
        d3.select(this).select("rect").attr("fill", "transparent");
      });

    // Metric name
    rows
      .append("text")
      .attr("x", -6)
      .attr("y", rowHeight / 2)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#94a3b8")
      .attr("font-size", "10px")
      .attr("font-family", "monospace")
      .text((d) => {
        const label = d.metric;
        return label.length > 18 ? label.slice(0, 17) + "\u2026" : label;
      });

    // Direction arrow
    rows
      .append("text")
      .attr("x", 0)
      .attr("y", rowHeight / 2)
      .attr("dominant-baseline", "middle")
      .attr("font-size", "12px")
      .attr("fill", (d) => directionColor(d.direction))
      .text((d) => DIRECTION_ARROWS[d.direction] || "");

    // Magnitude bar
    rows
      .append("rect")
      .attr("x", 16)
      .attr("y", (rowHeight - 14) / 2)
      .attr("width", (d) => x(Math.abs(d.magnitude)))
      .attr("height", 14)
      .attr("rx", 3)
      .attr("fill", (d) => magnitudeColor(d.magnitude) + "50")
      .attr("stroke", (d) => magnitudeColor(d.magnitude) + "30")
      .attr("stroke-width", 0.5);

    // Magnitude value
    rows
      .append("text")
      .attr("x", (d) => 16 + x(Math.abs(d.magnitude)) + 6)
      .attr("y", rowHeight / 2)
      .attr("dominant-baseline", "middle")
      .attr("fill", (d) => magnitudeColor(d.magnitude))
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .attr("font-weight", "bold")
      .text((d) => d.magnitude.toFixed(2));

    // Current value vs baseline (right side)
    rows
      .append("text")
      .attr("x", barWidth)
      .attr("y", rowHeight / 2 - 5)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#cbd5e1")
      .attr("font-size", "10px")
      .attr("font-family", "monospace")
      .text((d) => formatValue(d.current_value));

    rows
      .append("text")
      .attr("x", barWidth)
      .attr("y", rowHeight / 2 + 7)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#475569")
      .attr("font-size", "8px")
      .attr("font-family", "monospace")
      .text((d) => `baseline ${formatValue(d.baseline_mean)}`);

    // Severity dot based on magnitude
    rows
      .append("circle")
      .attr("cx", barWidth + 14)
      .attr("cy", rowHeight / 2)
      .attr("r", 3)
      .attr("fill", (d) => magnitudeColor(d.magnitude));
  }, [data, handleRowClick]);

  const selected = selectedMetric
    ? data.find((d) => d.metric === selectedMetric)
    : null;

  return (
    <div className="w-full h-full overflow-y-auto">
      <svg ref={svgRef} className="w-full" />
      {selected && <TrendDetailPanel entry={selected} />}
    </div>
  );
}

function TrendDetailPanel({ entry }: { entry: TrendEntry }) {
  const pctChange =
    entry.baseline_mean !== 0
      ? ((entry.current_value - entry.baseline_mean) / entry.baseline_mean) *
        100
      : 0;

  const color = magnitudeColor(entry.magnitude);
  const dirColor = directionColor(entry.direction);
  const arrow = DIRECTION_ARROWS[entry.direction] || "";

  // Scale for the comparison bar
  const maxVal = Math.max(
    Math.abs(entry.current_value),
    Math.abs(entry.baseline_mean)
  );

  const baselinePct = maxVal > 0 ? (entry.baseline_mean / maxVal) * 100 : 0;
  const currentPct = maxVal > 0 ? (entry.current_value / maxVal) * 100 : 0;

  return (
    <div
      style={{
        background: "rgba(12, 16, 24, 0.95)",
        border: "1px solid rgba(0, 240, 255, 0.08)",
        borderRadius: "6px",
        padding: "12px 16px",
        marginTop: "8px",
        fontFamily: "monospace",
        fontSize: "11px",
      }}
    >
      {/* Header row */}
      <div
        style={{
          display: "flex",
          alignItems: "center",
          gap: "8px",
          marginBottom: "10px",
        }}
      >
        <span style={{ color: dirColor, fontSize: "16px" }}>{arrow}</span>
        <span style={{ color: "#e2e8f0", fontSize: "13px", fontWeight: 600 }}>
          {entry.metric}
        </span>
        <span
          style={{
            color,
            fontSize: "9px",
            padding: "2px 6px",
            background: color + "18",
            borderRadius: "3px",
            marginLeft: "auto",
          }}
        >
          {severityLabel(entry.magnitude)}
        </span>
      </div>

      {/* Stats grid */}
      <div
        style={{
          display: "grid",
          gridTemplateColumns: "1fr 1fr 1fr",
          gap: "12px",
          marginBottom: "10px",
        }}
      >
        <div>
          <div style={{ color: "#475569", fontSize: "9px", marginBottom: "2px" }}>
            MAGNITUDE
          </div>
          <div style={{ color, fontWeight: "bold" }}>
            {entry.magnitude.toFixed(2)}
          </div>
        </div>
        <div>
          <div style={{ color: "#475569", fontSize: "9px", marginBottom: "2px" }}>
            CHANGE
          </div>
          <div style={{ color: dirColor, fontWeight: "bold" }}>
            {pctChange >= 0 ? "+" : ""}
            {pctChange.toFixed(1)}%
          </div>
        </div>
        <div>
          <div style={{ color: "#475569", fontSize: "9px", marginBottom: "2px" }}>
            DIRECTION
          </div>
          <div style={{ color: dirColor }}>
            {arrow} {entry.direction}
          </div>
        </div>
      </div>

      {/* Visual comparison bar */}
      <div style={{ marginBottom: "4px" }}>
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            marginBottom: "4px",
          }}
        >
          <span style={{ color: "#475569", fontSize: "9px" }}>
            BASELINE: {formatValue(entry.baseline_mean)}
          </span>
          <span style={{ color: "#94a3b8", fontSize: "9px" }}>
            CURRENT: {formatValue(entry.current_value)}
          </span>
        </div>
        <div
          style={{
            position: "relative",
            height: "16px",
            background: "rgba(255,255,255,0.03)",
            borderRadius: "3px",
            overflow: "hidden",
          }}
        >
          {/* Baseline bar */}
          <div
            style={{
              position: "absolute",
              top: "2px",
              left: 0,
              height: "5px",
              width: `${Math.abs(baselinePct)}%`,
              background: "#475569",
              borderRadius: "2px",
            }}
          />
          {/* Current bar */}
          <div
            style={{
              position: "absolute",
              bottom: "2px",
              left: 0,
              height: "5px",
              width: `${Math.abs(currentPct)}%`,
              background: dirColor,
              borderRadius: "2px",
              opacity: 0.8,
            }}
          />
        </div>
      </div>
    </div>
  );
}

function formatValue(v: number): string {
  if (Math.abs(v) >= 1_000_000) return `${(v / 1_000_000).toFixed(1)}M`;
  if (Math.abs(v) >= 1_000) return `${(v / 1_000).toFixed(1)}K`;
  if (Number.isInteger(v)) return v.toLocaleString();
  return v.toFixed(2);
}
