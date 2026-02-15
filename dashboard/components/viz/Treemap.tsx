"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";

export interface TreemapNode {
  name: string;
  value?: number;
  children?: TreemapNode[];
}

interface Props {
  data: TreemapNode;
  title?: string;
  onNodeClick?: (name: string) => void;
}

const TREEMAP_COLORS = [
  "#00f0ff",
  "#ff6eb4",
  "#06d6a0",
  "#ffe600",
  "#c084fc",
  "#ff8a00",
  "#00ff88",
  "#ff4757",
  "#2ec4b6",
  "#9d4edd",
];

export default function Treemap({ data, title, onNodeClick }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    name: string;
    value: number;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = Math.max(container.clientHeight, 300);
    const margin = { top: title ? 32 : 8, right: 4, bottom: 4, left: 4 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    if (title) {
      svg
        .append("text")
        .attr("x", fullWidth / 2)
        .attr("y", 18)
        .attr("text-anchor", "middle")
        .attr("fill", "#64748b")
        .attr("font-size", "11px")
        .attr("font-weight", "bold")
        .attr("letter-spacing", "0.1em")
        .text(title.toUpperCase());
    }

    const hierarchy = d3
      .hierarchy(data)
      .sum((d) => d.value || 0)
      .sort((a, b) => (b.value || 0) - (a.value || 0));

    const root = d3
      .treemap<TreemapNode>()
      .size([width, height])
      .padding(2)
      .round(true)(hierarchy);

    const colorScale = d3
      .scaleOrdinal<string>()
      .domain(
        (root.children || []).map((d) => d.data.name)
      )
      .range(TREEMAP_COLORS);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    const leaves = root.leaves();

    const cell = g
      .selectAll("g")
      .data(leaves)
      .join("g")
      .attr("transform", (d) => `translate(${d.x0},${d.y0})`);

    cell
      .append("rect")
      .attr("width", (d) => d.x1 - d.x0)
      .attr("height", (d) => d.y1 - d.y0)
      .attr("rx", 3)
      .attr("fill", (d) => {
        // Color by top-level parent
        let node = d;
        while (node.depth > 1 && node.parent) node = node.parent;
        return colorScale(node.data.name) + "40";
      })
      .attr("stroke", (d) => {
        let node = d;
        while (node.depth > 1 && node.parent) node = node.parent;
        return colorScale(node.data.name) + "60";
      })
      .attr("stroke-width", 0.5)
      .attr("cursor", onNodeClick ? "pointer" : "default")
      .on("mouseover", function (event, d) {
        d3.select(this).attr("stroke-width", 1.5);
        const [mx, my] = d3.pointer(event, container);
        setTooltip({
          x: mx,
          y: my,
          name: d.data.name,
          value: d.value || 0,
        });
      })
      .on("mouseout", function () {
        d3.select(this).attr("stroke-width", 0.5);
        setTooltip(null);
      })
      .on("click", (_, d) => onNodeClick?.(d.data.name));

    // Labels (only if cell is big enough)
    cell
      .append("text")
      .attr("x", 4)
      .attr("y", 12)
      .attr("fill", "#94a3b8")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text((d) => {
        const w = d.x1 - d.x0;
        const h = d.y1 - d.y0;
        if (w < 40 || h < 18) return "";
        const name = d.data.name;
        const maxChars = Math.floor(w / 6);
        return name.length > maxChars
          ? name.slice(0, maxChars - 1) + "\u2026"
          : name;
      });
  }, [data, title, onNodeClick]);

  return (
    <div ref={containerRef} className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
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
          <div className="text-[10px] text-slate-300 font-mono">
            {tooltip.name}
          </div>
          <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">
            {tooltip.value.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}
