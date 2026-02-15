"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";

export interface LineChartDatum {
  timestamp: string;
  value: number;
  series?: string;
}

interface Props {
  data: LineChartDatum[];
  title?: string;
  showArea?: boolean;
}

const SERIES_COLORS = [
  "#00f0ff",
  "#ff6eb4",
  "#06d6a0",
  "#ffe600",
  "#c084fc",
  "#ff8a00",
  "#00ff88",
  "#ff4757",
];

export default function LineChart({ data, title, showArea = false }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    timestamp: string;
    value: number;
    series?: string;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !data.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = Math.max(container.clientHeight, 250);
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

    // Group by series
    const seriesNames = [...new Set(data.map((d) => d.series || "default"))];
    const colorScale = d3
      .scaleOrdinal<string>()
      .domain(seriesNames)
      .range(SERIES_COLORS);

    const parseTime = (ts: string) => new Date(ts);
    const timeExtent = d3.extent(data, (d) => parseTime(d.timestamp)) as [
      Date,
      Date,
    ];
    const valueExtent = d3.extent(data, (d) => d.value) as [number, number];

    const x = d3.scaleTime().domain(timeExtent).range([0, width]);
    const y = d3
      .scaleLinear()
      .domain([Math.min(0, valueExtent[0]), valueExtent[1] * 1.1])
      .range([height, 0]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid lines
    g.append("g")
      .attr("class", "grid")
      .call(d3.axisLeft(y).ticks(5).tickSize(-width).tickFormat(() => ""))
      .selectAll("line")
      .attr("stroke", "#1e293b")
      .attr("stroke-dasharray", "2,4");
    g.selectAll(".grid .domain").remove();

    // X axis
    g.append("g")
      .attr("transform", `translate(0,${height})`)
      .call(d3.axisBottom(x).ticks(6))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    // Y axis
    g.append("g")
      .call(d3.axisLeft(y).ticks(5))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px");

    g.selectAll(".domain, .tick line").attr("stroke", "#1e293b");

    // Draw each series
    for (const seriesName of seriesNames) {
      const seriesData = data
        .filter((d) => (d.series || "default") === seriesName)
        .sort(
          (a, b) =>
            parseTime(a.timestamp).getTime() -
            parseTime(b.timestamp).getTime()
        );

      const color = colorScale(seriesName);

      const line = d3
        .line<LineChartDatum>()
        .x((d) => x(parseTime(d.timestamp)))
        .y((d) => y(d.value))
        .curve(d3.curveMonotoneX);

      if (showArea) {
        const area = d3
          .area<LineChartDatum>()
          .x((d) => x(parseTime(d.timestamp)))
          .y0(height)
          .y1((d) => y(d.value))
          .curve(d3.curveMonotoneX);

        g.append("path")
          .datum(seriesData)
          .attr("fill", color + "15")
          .attr("d", area);
      }

      g.append("path")
        .datum(seriesData)
        .attr("fill", "none")
        .attr("stroke", color)
        .attr("stroke-width", 1.5)
        .attr("d", line);

      // Dots for hover
      g.selectAll(`circle.series-${seriesName}`)
        .data(seriesData)
        .join("circle")
        .attr("class", `series-${seriesName}`)
        .attr("cx", (d) => x(parseTime(d.timestamp)))
        .attr("cy", (d) => y(d.value))
        .attr("r", 3)
        .attr("fill", color)
        .attr("opacity", 0)
        .on("mouseover", function (event, d) {
          d3.select(this).attr("opacity", 1).attr("r", 5);
          const [mx, my] = d3.pointer(event, container);
          setTooltip({
            x: mx,
            y: my,
            timestamp: d.timestamp,
            value: d.value,
            series: d.series,
          });
        })
        .on("mouseout", function () {
          d3.select(this).attr("opacity", 0).attr("r", 3);
          setTooltip(null);
        });
    }

    // Legend (if multi-series)
    if (seriesNames.length > 1 && seriesNames[0] !== "default") {
      const legend = svg
        .append("g")
        .attr(
          "transform",
          `translate(${margin.left + 10},${margin.top - 6})`
        );

      seriesNames.forEach((name, i) => {
        const lg = legend
          .append("g")
          .attr("transform", `translate(${i * 90},0)`);
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
          .text(name);
      });
    }
  }, [data, title, showArea]);

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
          {tooltip.series && (
            <div className="text-[10px] text-slate-500">{tooltip.series}</div>
          )}
          <div className="text-[10px] text-slate-400 font-mono">
            {new Date(tooltip.timestamp).toLocaleString()}
          </div>
          <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">
            {tooltip.value.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}
