"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import type { CooccurrenceData } from "@/lib/api";

interface Props {
  data: CooccurrenceData;
  onTypeChange?: (typeA: string, typeB: string) => void;
}

const ENTITY_TYPES = [
  "Game",
  "Platform",
  "Member",
  "Device",
  "Currency",
  "Provider",
  "VipGroup",
  "Affiliate",
  "Error",
  "Popup",
];

const TYPE_PAIRS = ENTITY_TYPES.flatMap((a, i) =>
  ENTITY_TYPES.slice(i).map((b) => `${a} x ${b}`)
);

export default function CooccurrenceHeatmap({ data, onTypeChange }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    entityA: string;
    entityB: string;
    count: number;
    pmi: number;
  } | null>(null);

  const selectedPair = `${data.entity_type_a} x ${data.entity_type_b}`;

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !data.pairs.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = container.clientHeight - 44; // account for selector
    const margin = { top: 60, right: 20, bottom: 20, left: 80 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    // Extract unique entities for axes
    const entitiesA = [...new Set(data.pairs.map((e) => e.entity_a))];
    const entitiesB = [...new Set(data.pairs.map((e) => e.entity_b))];

    const cellW = Math.min(
      Math.max(width / entitiesB.length, 12),
      40
    );
    const cellH = Math.min(
      Math.max(height / entitiesA.length, 12),
      40
    );

    const actualWidth = cellW * entitiesB.length;
    const actualHeight = cellH * entitiesA.length;

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // PMI color scale: red (negative) -> dark (0) -> cyan (positive)
    const pmiExtent = d3.extent(data.pairs, (e) => e.pmi ?? 0) as [number, number];
    const maxAbs = Math.max(Math.abs(pmiExtent[0] || 0), Math.abs(pmiExtent[1] || 0), 1);

    const colorScale = d3
      .scaleLinear<string>()
      .domain([-maxAbs, 0, maxAbs])
      .range(["#ff4757", "#111827", "#00f0ff"])
      .clamp(true);

    // Build lookup for fast access
    const pmiMap = new Map<string, { count: number; pmi: number }>();
    data.pairs.forEach((e) => {
      pmiMap.set(`${e.entity_a}|${e.entity_b}`, {
        count: e.count,
        pmi: e.pmi ?? 0,
      });
    });

    // Draw cells
    entitiesA.forEach((a, i) => {
      entitiesB.forEach((b, j) => {
        const val = pmiMap.get(`${a}|${b}`);
        if (!val) return;

        g.append("rect")
          .attr("x", j * cellW)
          .attr("y", i * cellH)
          .attr("width", cellW - 1)
          .attr("height", cellH - 1)
          .attr("rx", 2)
          .attr("fill", colorScale(val.pmi))
          .attr("opacity", 0.85)
          .attr("cursor", "pointer")
          .on("mouseover", function (event) {
            d3.select(this).attr("stroke", "#fff").attr("stroke-width", 1.5);
            const [mx, my] = d3.pointer(event, container);
            setTooltip({
              x: mx,
              y: my,
              entityA: a,
              entityB: b,
              count: val.count,
              pmi: val.pmi,
            });
          })
          .on("mouseout", function () {
            d3.select(this).attr("stroke", "none");
            setTooltip(null);
          });
      });
    });

    // X axis labels (top)
    g.selectAll(".x-label")
      .data(entitiesB)
      .join("text")
      .attr("class", "x-label")
      .attr("x", (_, i) => i * cellW + cellW / 2)
      .attr("y", -6)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr(
        "transform",
        (_, i) => `rotate(-45, ${i * cellW + cellW / 2}, -6)`
      )
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text((d) => (d.length > 12 ? d.slice(0, 11) + "\u2026" : d));

    // Y axis labels (left)
    g.selectAll(".y-label")
      .data(entitiesA)
      .join("text")
      .attr("class", "y-label")
      .attr("x", -6)
      .attr("y", (_, i) => i * cellH + cellH / 2)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text((d) => (d.length > 12 ? d.slice(0, 11) + "\u2026" : d));

    // Color legend
    const legendWidth = Math.min(actualWidth, 160);
    const legendG = svg
      .append("g")
      .attr(
        "transform",
        `translate(${margin.left + actualWidth / 2 - legendWidth / 2},${
          margin.top + actualHeight + 8
        })`
      );

    const legendScale = d3
      .scaleLinear()
      .domain([-maxAbs, maxAbs])
      .range([0, legendWidth]);

    const legendAxis = d3
      .axisBottom(legendScale)
      .ticks(5)
      .tickFormat(d3.format(".1f"));

    // Gradient
    const gradId = "pmi-gradient";
    const defs = svg.append("defs");
    const linearGrad = defs
      .append("linearGradient")
      .attr("id", gradId);
    linearGrad
      .append("stop")
      .attr("offset", "0%")
      .attr("stop-color", "#ff4757");
    linearGrad
      .append("stop")
      .attr("offset", "50%")
      .attr("stop-color", "#111827");
    linearGrad
      .append("stop")
      .attr("offset", "100%")
      .attr("stop-color", "#00f0ff");

    legendG
      .append("rect")
      .attr("width", legendWidth)
      .attr("height", 8)
      .attr("rx", 4)
      .attr("fill", `url(#${gradId})`);

    legendG
      .append("g")
      .attr("transform", "translate(0,8)")
      .call(legendAxis)
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "8px");
    legendG.selectAll(".domain, line").attr("stroke", "#334155");

    legendG
      .append("text")
      .attr("x", legendWidth / 2)
      .attr("y", -4)
      .attr("text-anchor", "middle")
      .attr("fill", "#475569")
      .attr("font-size", "8px")
      .text("PMI");
  }, [data]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      {/* Type pair selector */}
      <div
        className="flex flex-wrap gap-1 px-3 py-2 overflow-x-auto shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        {TYPE_PAIRS.slice(0, 10).map((pair) => {
          const [a, b] = pair.split(" x ");
          return (
            <button
              key={pair}
              onClick={() => onTypeChange?.(a, b)}
              className="text-[9px] font-bold tracking-wider uppercase px-2 py-1 rounded-lg transition-all whitespace-nowrap"
              style={{
                color: selectedPair === pair ? "#00f0ff" : "#475569",
                background:
                  selectedPair === pair
                    ? "rgba(0, 240, 255, 0.08)"
                    : "transparent",
                border: `1px solid ${
                  selectedPair === pair
                    ? "rgba(0, 240, 255, 0.2)"
                    : "rgba(71, 85, 105, 0.15)"
                }`,
              }}
            >
              {pair}
            </button>
          );
        })}
      </div>

      <svg ref={svgRef} className="w-full" />

      {/* Tooltip */}
      {tooltip && (
        <div
          className="absolute pointer-events-none rounded-lg px-3 py-2 text-sm backdrop-blur-md z-10"
          style={{
            left: tooltip.x + 12,
            top: tooltip.y - 10,
            background: "rgba(6, 8, 13, 0.9)",
            border: "1px solid rgba(0, 240, 255, 0.2)",
            boxShadow: "0 0 20px rgba(0, 240, 255, 0.1)",
          }}
        >
          <div className="text-[10px] text-slate-400 font-mono">
            {tooltip.entityA} &times; {tooltip.entityB}
          </div>
          <div className="flex items-center gap-3 mt-1">
            <span className="text-[10px] text-slate-500">
              count:{" "}
              <span className="text-slate-300 font-mono">
                {tooltip.count.toLocaleString()}
              </span>
            </span>
            <span className="text-[10px] text-slate-500">
              PMI:{" "}
              <span
                className="font-mono font-bold"
                style={{
                  color:
                    tooltip.pmi > 0
                      ? "#00f0ff"
                      : tooltip.pmi < 0
                      ? "#ff4757"
                      : "#64748b",
                }}
              >
                {tooltip.pmi.toFixed(3)}
              </span>
            </span>
          </div>
        </div>
      )}
    </div>
  );
}
