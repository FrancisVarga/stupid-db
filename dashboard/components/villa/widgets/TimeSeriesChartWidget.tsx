"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import { adaptTimeSeriesData, type TimeSeriesPoint } from "@/lib/villa/adapters/time-series";

const SERIES_COLORS = [
  "#00f0ff", // cyan
  "#ff6eb4", // pink
  "#06d6a0", // mint
  "#ffe600", // yellow
  "#c084fc", // purple
  "#ff8a00", // orange
  "#00ff88", // green
  "#ff4757", // red
];

interface TimeSeriesChartWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function TimeSeriesChartWidget({ data, dimensions }: TimeSeriesChartWidgetProps) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    time: string;
    value: number;
    series?: string;
  } | null>(null);

  const points = adaptTimeSeriesData(data);

  useEffect(() => {
    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    if (!svgRef.current || points.length === 0) return;

    const { width: fullWidth, height: fullHeight } = dimensions;
    const margin = { top: 12, right: 16, bottom: 32, left: 48 };
    const width = fullWidth - margin.left - margin.right;
    const height = fullHeight - margin.top - margin.bottom;

    if (width <= 0 || height <= 0) return;

    svg.attr("width", fullWidth).attr("height", fullHeight);

    // Group by series
    const seriesNames = [...new Set(points.map((d) => d.series ?? "default"))];
    const colorScale = d3.scaleOrdinal<string>().domain(seriesNames).range(SERIES_COLORS);

    // Scales
    const timeExtent = d3.extent(points, (d) => d.timestamp) as [number, number];
    const valueExtent = d3.extent(points, (d) => d.value) as [number, number];

    const x = d3.scaleTime().domain(timeExtent).range([0, width]);
    const y = d3
      .scaleLinear()
      .domain([Math.min(0, valueExtent[0]), valueExtent[1] * 1.1])
      .range([height, 0]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid lines (horizontal)
    g.append("g")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".domain").remove();

    // X axis
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(Math.min(6, Math.floor(width / 80))))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    // Y axis
    g.append("g")
      .call(d3.axisLeft(y).ticks(5))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line:not([stroke-dasharray])").attr("stroke", "#1e293b");

    // Draw each series
    for (const seriesName of seriesNames) {
      const seriesData = points
        .filter((d) => (d.series ?? "default") === seriesName)
        .sort((a, b) => a.timestamp - b.timestamp);

      const color = colorScale(seriesName);

      // Area fill
      const area = d3
        .area<TimeSeriesPoint>()
        .x((d) => x(d.timestamp))
        .y0(height)
        .y1((d) => y(d.value))
        .curve(d3.curveMonotoneX);

      g.append("path")
        .datum(seriesData)
        .attr("fill", color + "12")
        .attr("d", area);

      // Line
      const line = d3
        .line<TimeSeriesPoint>()
        .x((d) => x(d.timestamp))
        .y((d) => y(d.value))
        .curve(d3.curveMonotoneX);

      g.append("path")
        .datum(seriesData)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", 1.5)
        .attr("filter", `drop-shadow(0 0 4px ${color}40)`)
        .attr("d", line);

      // Hover dots
      g.selectAll(`.dot-${seriesName}`)
        .data(seriesData)
        .join("circle")
        .attr("class", `dot-${seriesName}`)
        .attr("cx", (d) => x(d.timestamp))
        .attr("cy", (d) => y(d.value))
        .attr("r", 3)
        .attr("fill", color)
        .attr("opacity", 0)
        .on("mouseover", function (event, d) {
          d3.select(this).attr("opacity", 1).attr("r", 5);
          const svgEl = svgRef.current;
          if (!svgEl) return;
          const rect = svgEl.getBoundingClientRect();
          setTooltip({
            x: event.clientX - rect.left,
            y: event.clientY - rect.top,
            time: new Date(d.timestamp).toLocaleString(),
            value: d.value,
            series: d.series,
          });
        })
        .on("mouseout", function () {
          d3.select(this).attr("opacity", 0).attr("r", 3);
          setTooltip(null);
        });
    }

    // Legend (only for multi-series, non-default)
    if (seriesNames.length > 1 && seriesNames[0] !== "default") {
      const legend = svg
        .append("g")
        .attr("transform", `translate(${margin.left + 8},${margin.top})`);

      seriesNames.forEach((name, i) => {
        const lg = legend.append("g").attr("transform", `translate(${i * 90},0)`);
        lg.append("rect")
          .attr("width", 10)
          .attr("height", 3)
          .attr("rx", 1.5)
          .attr("fill", colorScale(name));
        lg.append("text")
          .attr("x", 14)
          .attr("y", 3)
          .attr("fill", "#64748b")
          .attr("font-size", "9px")
          .attr("font-family", "monospace")
          .text(name.length > 12 ? name.slice(0, 11) + "\u2026" : name);
      });
    }
  }, [points, dimensions]);

  // Empty state
  if (points.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No data</span>
      </div>
    );
  }

  return (
    <div className="relative w-full h-full">
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
          {tooltip.series && (
            <div className="text-[10px] text-slate-500">{tooltip.series}</div>
          )}
          <div className="text-[10px] text-slate-400 font-mono">{tooltip.time}</div>
          <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">
            {tooltip.value.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}
