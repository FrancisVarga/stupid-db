"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";

export interface ScatterDatum {
  x: number;
  y: number;
  label?: string;
  cluster?: number;
  size?: number;
}

interface Props {
  data: ScatterDatum[];
  title?: string;
  onPointClick?: (label: string) => void;
}

const CLUSTER_COLORS = [
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
  "#f97316",
  "#38bdf8",
];

export default function ScatterPlot({ data, title, onPointClick }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    datum: ScatterDatum;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = Math.max(container.clientHeight, 300);
    const margin = { top: title ? 36 : 16, right: 20, bottom: 40, left: 50 };
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

    const xExtent = d3.extent(data, (d) => d.x) as [number, number];
    const yExtent = d3.extent(data, (d) => d.y) as [number, number];
    const xPad = (xExtent[1] - xExtent[0]) * 0.05 || 1;
    const yPad = (yExtent[1] - yExtent[0]) * 0.05 || 1;

    const x = d3
      .scaleLinear()
      .domain([xExtent[0] - xPad, xExtent[1] + xPad])
      .range([0, width]);
    const y = d3
      .scaleLinear()
      .domain([yExtent[0] - yPad, yExtent[1] + yPad])
      .range([height, 0]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Clip path
    g.append("defs")
      .append("clipPath")
      .attr("id", "scatter-clip")
      .append("rect")
      .attr("width", width)
      .attr("height", height);

    const plotArea = g.append("g").attr("clip-path", "url(#scatter-clip)");

    // Axes
    const xAxis = g
      .append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(6));
    xAxis.selectAll("text").attr("fill", "#475569").attr("font-size", "9px");

    const yAxis = g.append("g").call(d3.axisLeft(y).ticks(6));
    yAxis.selectAll("text").attr("fill", "#475569").attr("font-size", "9px");

    g.selectAll(".domain, .tick line").attr("stroke", "#1e293b");

    // Grid
    g.append("g")
      .call(d3.axisLeft(y).ticks(6).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").filter((_, i) => i > 0).remove();

    // Points
    plotArea
      .selectAll("circle")
      .data(data)
      .join("circle")
      .attr("cx", (d) => x(d.x))
      .attr("cy", (d) => y(d.y))
      .attr("r", (d) => d.size || 4)
      .attr("fill", (d) => {
        if (d.cluster !== undefined) {
          return CLUSTER_COLORS[d.cluster % CLUSTER_COLORS.length] + "80";
        }
        return "#00f0ff80";
      })
      .attr("stroke", (d) => {
        if (d.cluster !== undefined) {
          return CLUSTER_COLORS[d.cluster % CLUSTER_COLORS.length];
        }
        return "#00f0ff";
      })
      .attr("stroke-width", 0.5)
      .attr("cursor", onPointClick ? "pointer" : "default")
      .on("mouseover", function (event, d) {
        d3.select(this).attr("r", (d.size || 4) + 2).attr("stroke-width", 1.5);
        const [mx, my] = d3.pointer(event, container);
        setTooltip({ x: mx, y: my, datum: d });
      })
      .on("mouseout", function (_, d) {
        d3.select(this).attr("r", d.size || 4).attr("stroke-width", 0.5);
        setTooltip(null);
      })
      .on("click", (_, d) => {
        if (d.label) onPointClick?.(d.label);
      });

    // Zoom
    const zoom = d3
      .zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.5, 10])
      .on("zoom", (event) => {
        const newX = event.transform.rescaleX(x);
        const newY = event.transform.rescaleY(y);
        xAxis.call(d3.axisBottom(newX).ticks(6));
        yAxis.call(d3.axisLeft(newY).ticks(6));
        xAxis.selectAll("text").attr("fill", "#475569").attr("font-size", "9px");
        yAxis.selectAll("text").attr("fill", "#475569").attr("font-size", "9px");
        g.selectAll(".domain, .tick line").attr("stroke", "#1e293b");
        plotArea
          .selectAll("circle")
          .attr("cx", (d) => newX((d as ScatterDatum).x))
          .attr("cy", (d) => newY((d as ScatterDatum).y));
      });

    svg.call(zoom);
  }, [data, title, onPointClick]);

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
          {tooltip.datum.label && (
            <div className="text-[10px] text-slate-300 font-mono">
              {tooltip.datum.label}
            </div>
          )}
          <div className="text-[10px] text-slate-500 mt-0.5">
            x: <span className="text-slate-300 font-mono">{tooltip.datum.x.toFixed(2)}</span>{" "}
            y: <span className="text-slate-300 font-mono">{tooltip.datum.y.toFixed(2)}</span>
          </div>
          {tooltip.datum.cluster !== undefined && (
            <div className="text-[10px] text-slate-500 mt-0.5">
              cluster:{" "}
              <span
                className="font-mono font-bold"
                style={{
                  color:
                    CLUSTER_COLORS[
                      tooltip.datum.cluster % CLUSTER_COLORS.length
                    ],
                }}
              >
                {tooltip.datum.cluster}
              </span>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
