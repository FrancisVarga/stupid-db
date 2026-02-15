"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { AnomalyEntry } from "@/lib/api";

const ENTITY_COLORS: Record<string, string> = {
  Member: "#00d4ff",
  Device: "#00ff88",
  Platform: "#ff8a00",
  Currency: "#ffe600",
  VipGroup: "#c084fc",
  Affiliate: "#ff6eb4",
  Game: "#06d6a0",
  Error: "#ff4757",
  Popup: "#9d4edd",
  Provider: "#2ec4b6",
};

function classificationColor(score: number): string {
  if (score >= 0.7) return "#ff4757"; // HighlyAnomalous — red
  if (score >= 0.5) return "#ff8a00"; // Anomalous — orange
  if (score >= 0.3) return "#ffe600"; // Mild — yellow
  return "#06d6a0"; // Normal — green
}

function classificationLabel(score: number): string {
  if (score >= 0.7) return "CRITICAL";
  if (score >= 0.5) return "ANOMALOUS";
  if (score >= 0.3) return "MILD";
  return "NORMAL";
}

interface Props {
  data: AnomalyEntry[];
}

export default function AnomalyChart({ data }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const detailRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!svgRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = svgRef.current.parentElement!;
    const width = container.clientWidth;
    const barHeight = 28;
    const margin = { top: 8, right: 90, bottom: 8, left: 140 };
    const height = Math.max(
      data.length * barHeight + margin.top + margin.bottom,
      200,
    );

    svg.attr("width", width).attr("height", height);

    const x = d3
      .scaleLinear()
      .domain([0, 1])
      .range([0, width - margin.left - margin.right]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Threshold lines
    const thresholds = [
      { value: 0.3, color: "#ffe60040" },
      { value: 0.5, color: "#ff8a0040" },
      { value: 0.7, color: "#ff475740" },
    ];

    for (const t of thresholds) {
      g.append("line")
        .attr("x1", x(t.value))
        .attr("x2", x(t.value))
        .attr("y1", 0)
        .attr("y2", data.length * barHeight)
        .attr("stroke", t.color)
        .attr("stroke-dasharray", "3,3");
    }

    // Bars — use data-id attribute for React click handling
    const bars = g
      .selectAll("g.bar")
      .data(data)
      .join("g")
      .attr("class", "bar")
      .attr("data-anomaly-id", (d) => d.id)
      .attr("transform", (_, i) => `translate(0,${i * barHeight})`)
      .style("cursor", "pointer");

    // Invisible full-width hit area
    bars
      .append("rect")
      .attr("x", -margin.left)
      .attr("width", width)
      .attr("height", barHeight)
      .attr("fill", "transparent")
      .attr("class", "hit-area");

    // Hover highlight via D3
    bars
      .on("mouseenter", function () {
        d3.select(this).select(".hit-area").attr("fill", "rgba(0, 240, 255, 0.04)");
      })
      .on("mouseleave", function () {
        d3.select(this).select(".hit-area").attr("fill", "transparent");
      });

    // Visible score bar
    bars
      .append("rect")
      .attr("width", (d) => x(d.score))
      .attr("height", barHeight - 4)
      .attr("y", 1)
      .attr("rx", 3)
      .attr("fill", (d) => classificationColor(d.score) + "50")
      .attr("stroke", (d) => classificationColor(d.score) + "80")
      .attr("stroke-width", 0.5)
      .style("pointer-events", "none");

    // Entity type dot
    bars
      .append("circle")
      .attr("cx", -16)
      .attr("cy", barHeight / 2 - 1)
      .attr("r", 3)
      .attr("fill", (d) => ENTITY_COLORS[d.entity_type] || "#888")
      .style("pointer-events", "none");

    // Labels (left)
    bars
      .append("text")
      .attr("x", -24)
      .attr("y", barHeight / 2)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#94a3b8")
      .attr("font-size", "10px")
      .attr("font-family", "monospace")
      .style("pointer-events", "none")
      .text((d) => {
        const label = d.key;
        return label.length > 16 ? label.slice(0, 15) + "\u2026" : label;
      });

    // Score + classification (right)
    bars
      .append("text")
      .attr("x", (d) => x(d.score) + 6)
      .attr("y", barHeight / 2)
      .attr("dominant-baseline", "middle")
      .attr("fill", (d) => classificationColor(d.score))
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .style("pointer-events", "none")
      .text(
        (d) => `${d.score.toFixed(3)} ${classificationLabel(d.score)}`,
      );
  }, [data]);

  // React-level click handler on SVG — bypasses D3 closure issues
  // and works reliably with React Compiler.
  function handleSvgClick(e: React.MouseEvent<SVGSVGElement>) {
    const target = e.target as SVGElement;
    const barGroup = target.closest("g.bar");
    if (!barGroup) return;
    const id = barGroup.getAttribute("data-anomaly-id");
    if (!id) return;
    setSelectedId((prev) => (prev === id ? null : id));
  }

  const selected = selectedId ? data.find((d) => d.id === selectedId) : null;

  // Scroll detail panel into view when selected
  useEffect(() => {
    if (selected && detailRef.current) {
      detailRef.current.scrollIntoView({ behavior: "smooth", block: "nearest" });
    }
  }, [selected]);

  return (
    <div className="w-full h-full overflow-y-auto">
      <svg ref={svgRef} className="w-full" onClick={handleSvgClick} />
      {selected && (
        <div ref={detailRef}>
          <DetailPanel entry={selected} />
        </div>
      )}
    </div>
  );
}

// ── Detail Panel ──────────────────────────────────────────────────

function DetailPanel({ entry }: { entry: AnomalyEntry }) {
  const color = classificationColor(entry.score);
  const label = classificationLabel(entry.score);
  const maxFeatureValue =
    entry.features && entry.features.length > 0
      ? Math.max(...entry.features.map((f) => f.value))
      : 1;

  return (
    <div
      className="mx-4 mt-2 mb-4 rounded-lg border p-4"
      style={{
        background: "#0c1018",
        borderColor: color + "40",
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-2">
          <span
            className="inline-block w-2 h-2 rounded-full"
            style={{ background: ENTITY_COLORS[entry.entity_type] || "#888" }}
          />
          <span
            className="font-mono text-sm"
            style={{ color: "#e2e8f0" }}
          >
            {entry.key}
          </span>
        </div>
        <div className="flex items-center gap-3">
          {entry.cluster_id != null && (
            <span
              className="text-xs font-mono px-2 py-0.5 rounded"
              style={{ background: "#1e293b", color: "#94a3b8" }}
            >
              cluster {entry.cluster_id}
            </span>
          )}
          <span
            className="text-xs font-mono font-bold px-2 py-0.5 rounded"
            style={{ background: color + "20", color }}
          >
            {label} {entry.score.toFixed(3)}
          </span>
        </div>
      </div>

      {/* Feature breakdown */}
      {entry.features && entry.features.length > 0 && (
        <div className="space-y-1">
          <span
            className="text-xs font-mono block mb-1"
            style={{ color: "#64748b" }}
          >
            Feature Breakdown
          </span>
          {entry.features.map((f) => (
            <div key={f.name} className="flex items-center gap-2">
              <span
                className="text-xs font-mono w-40 text-right shrink-0"
                style={{ color: "#94a3b8" }}
              >
                {f.name}
              </span>
              <div
                className="flex-1 h-3 rounded-sm overflow-hidden"
                style={{ background: "#1e293b" }}
              >
                <div
                  className="h-full rounded-sm"
                  style={{
                    width: `${maxFeatureValue > 0 ? (f.value / maxFeatureValue) * 100 : 0}%`,
                    background: "#00f0ff60",
                    minWidth: f.value > 0 ? "2px" : "0",
                  }}
                />
              </div>
              <span
                className="text-xs font-mono w-16 text-right shrink-0"
                style={{ color: "#64748b" }}
              >
                {f.value % 1 === 0 ? f.value.toFixed(0) : f.value.toFixed(2)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
