"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";

export interface BarChartDatum {
  label: string;
  value: number;
  color?: string;
}

interface Props {
  data: BarChartDatum[];
  title?: string;
  orientation?: "vertical" | "horizontal";
  onBarClick?: (label: string) => void;
}

export default function BarChart({
  data,
  title,
  orientation = "horizontal",
  onBarClick,
}: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    label: string;
    value: number;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = Math.max(container.clientHeight, 200);

    if (orientation === "horizontal") {
      // Horizontal bars (labels on left, bars go right)
      const barHeight = 22;
      const margin = { top: title ? 28 : 8, right: 60, bottom: 8, left: 120 };
      const height = Math.max(
        data.length * barHeight + margin.top + margin.bottom,
        200
      );

      svg.attr("width", fullWidth).attr("height", height);

      if (title) {
        svg
          .append("text")
          .attr("x", fullWidth / 2)
          .attr("y", 16)
          .attr("text-anchor", "middle")
          .attr("fill", "#64748b")
          .attr("font-size", "11px")
          .attr("font-weight", "bold")
          .attr("letter-spacing", "0.1em")
          .text(title.toUpperCase());
      }

      const maxVal = d3.max(data, (d) => d.value) || 1;
      const x = d3
        .scaleLinear()
        .domain([0, maxVal])
        .range([0, fullWidth - margin.left - margin.right]);

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
        .attr("width", 0)
        .attr("height", barHeight - 3)
        .attr("rx", 3)
        .attr("fill", (d) => (d.color || "#00f0ff") + "50")
        .attr("stroke", (d) => (d.color || "#00f0ff") + "30")
        .attr("stroke-width", 0.5)
        .attr("cursor", onBarClick ? "pointer" : "default")
        .on("mouseover", function (event, d) {
          d3.select(this).attr("fill", (d.color || "#00f0ff") + "80");
          const [mx, my] = d3.pointer(event, container);
          setTooltip({ x: mx, y: my, label: d.label, value: d.value });
        })
        .on("mouseout", function (_, d) {
          d3.select(this).attr("fill", (d.color || "#00f0ff") + "50");
          setTooltip(null);
        })
        .on("click", (_, d) => onBarClick?.(d.label))
        .transition()
        .duration(600)
        .attr("width", (d) => x(d.value));

      bars
        .append("text")
        .attr("x", -6)
        .attr("y", barHeight / 2)
        .attr("text-anchor", "end")
        .attr("dominant-baseline", "middle")
        .attr("fill", "#94a3b8")
        .attr("font-size", "10px")
        .attr("font-family", "monospace")
        .text((d) =>
          d.label.length > 16 ? d.label.slice(0, 15) + "\u2026" : d.label
        );

      bars
        .append("text")
        .attr("x", (d) => x(d.value) + 6)
        .attr("y", barHeight / 2)
        .attr("dominant-baseline", "middle")
        .attr("fill", "#64748b")
        .attr("font-size", "9px")
        .attr("font-family", "monospace")
        .text((d) => d.value.toLocaleString());
    } else {
      // Vertical bars
      const margin = { top: title ? 28 : 8, right: 20, bottom: 60, left: 50 };
      const width = fullWidth - margin.left - margin.right;
      const height = fullHeight - margin.top - margin.bottom;

      svg.attr("width", fullWidth).attr("height", fullHeight);

      if (title) {
        svg
          .append("text")
          .attr("x", fullWidth / 2)
          .attr("y", 16)
          .attr("text-anchor", "middle")
          .attr("fill", "#64748b")
          .attr("font-size", "11px")
          .attr("font-weight", "bold")
          .attr("letter-spacing", "0.1em")
          .text(title.toUpperCase());
      }

      const x = d3
        .scaleBand()
        .domain(data.map((d) => d.label))
        .range([0, width])
        .padding(0.3);

      const maxVal = d3.max(data, (d) => d.value) || 1;
      const y = d3.scaleLinear().domain([0, maxVal]).range([height, 0]);

      const g = svg
        .append("g")
        .attr("transform", `translate(${margin.left},${margin.top})`);

      // Y axis
      g.append("g")
        .call(d3.axisLeft(y).ticks(5))
        .selectAll("text")
        .attr("fill", "#475569")
        .attr("font-size", "9px");
      g.selectAll(".domain, line").attr("stroke", "#1e293b");

      // X axis
      g.append("g")
        .attr("transform", `translate(0,${height})`)
        .call(d3.axisBottom(x))
        .selectAll("text")
        .attr("fill", "#475569")
        .attr("font-size", "9px")
        .attr("transform", "rotate(-45)")
        .attr("text-anchor", "end");

      g.selectAll("rect.bar")
        .data(data)
        .join("rect")
        .attr("class", "bar")
        .attr("x", (d) => x(d.label)!)
        .attr("width", x.bandwidth())
        .attr("y", height)
        .attr("height", 0)
        .attr("rx", 2)
        .attr("fill", (d) => (d.color || "#00f0ff") + "50")
        .attr("stroke", (d) => (d.color || "#00f0ff") + "30")
        .attr("stroke-width", 0.5)
        .attr("cursor", onBarClick ? "pointer" : "default")
        .on("mouseover", function (event, d) {
          d3.select(this).attr("fill", (d.color || "#00f0ff") + "80");
          const [mx, my] = d3.pointer(event, container);
          setTooltip({ x: mx, y: my, label: d.label, value: d.value });
        })
        .on("mouseout", function (_, d) {
          d3.select(this).attr("fill", (d.color || "#00f0ff") + "50");
          setTooltip(null);
        })
        .on("click", (_, d) => onBarClick?.(d.label))
        .transition()
        .duration(600)
        .attr("y", (d) => y(d.value))
        .attr("height", (d) => height - y(d.value));
    }
  }, [data, title, orientation, onBarClick]);

  return (
    <div ref={containerRef} className="relative w-full h-full overflow-y-auto">
      <svg ref={svgRef} className="w-full" />
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
            {tooltip.label}
          </div>
          <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">
            {tooltip.value.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}
