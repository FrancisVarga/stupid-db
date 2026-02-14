"use client";

import { useEffect, useRef } from "react";
import * as d3 from "d3";
import type { DegreeEntry } from "@/lib/api";

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

interface Props {
  data: DegreeEntry[];
}

export default function DegreeChart({ data }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = svgRef.current.parentElement!;
    const width = container.clientWidth;
    const barHeight = 22;
    const margin = { top: 8, right: 50, bottom: 8, left: 140 };
    const height = Math.max(
      data.length * barHeight + margin.top + margin.bottom,
      200
    );

    svg.attr("width", width).attr("height", height);

    const maxDegree = d3.max(data, (d) => d.total) || 1;
    const x = d3
      .scaleLinear()
      .domain([0, maxDegree])
      .range([0, width - margin.left - margin.right]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    const bars = g
      .selectAll("g.bar")
      .data(data)
      .join("g")
      .attr("class", "bar")
      .attr("transform", (_, i) => `translate(0,${i * barHeight})`);

    bars
      .append("rect")
      .attr("width", (d) => x(d.total))
      .attr("height", barHeight - 3)
      .attr("rx", 3)
      .attr("fill", (d) => {
        const color = ENTITY_COLORS[d.entity_type] || "#888";
        return color + "60";
      })
      .attr("stroke", (d) => {
        const color = ENTITY_COLORS[d.entity_type] || "#888";
        return color + "40";
      })
      .attr("stroke-width", 0.5);

    // Labels
    bars
      .append("text")
      .attr("x", -6)
      .attr("y", barHeight / 2)
      .attr("text-anchor", "end")
      .attr("dominant-baseline", "middle")
      .attr("fill", "#94a3b8")
      .attr("font-size", "10px")
      .attr("font-family", "monospace")
      .text((d) => {
        const label = d.key.split(":").slice(1).join(":");
        return label.length > 18 ? label.slice(0, 17) + "\u2026" : label;
      });

    // Degree values
    bars
      .append("text")
      .attr("x", (d) => x(d.total) + 6)
      .attr("y", barHeight / 2)
      .attr("dominant-baseline", "middle")
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text((d) => d.total.toString());
  }, [data]);

  return (
    <div className="w-full h-full overflow-y-auto">
      <svg ref={svgRef} className="w-full" />
    </div>
  );
}
