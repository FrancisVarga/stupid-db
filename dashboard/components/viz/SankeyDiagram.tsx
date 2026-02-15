"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import {
  sankey as d3Sankey,
  sankeyLinkHorizontal,
} from "d3-sankey";

export interface SankeyNodeDatum {
  id: string;
  label: string;
}

export interface SankeyLinkDatum {
  source: string;
  target: string;
  value: number;
}

interface Props {
  nodes: SankeyNodeDatum[];
  links: SankeyLinkDatum[];
  title?: string;
  onLinkClick?: (source: string, target: string) => void;
}

const NODE_COLORS = [
  "#00f0ff",
  "#ff6eb4",
  "#06d6a0",
  "#ffe600",
  "#c084fc",
  "#ff8a00",
  "#00ff88",
  "#ff4757",
];

// d3-sankey adds x0/y0/x1/y1/value to nodes and width to links at runtime
interface SNode {
  id: string;
  label: string;
  x0?: number;
  y0?: number;
  x1?: number;
  y1?: number;
  value?: number;
}

interface SLink {
  source: SNode;
  target: SNode;
  value: number;
  width?: number;
  y0?: number;
  y1?: number;
}

export default function SankeyDiagram({
  nodes,
  links,
  title,
  onLinkClick,
}: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    source: string;
    target: string;
    value: number;
  } | null>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || !nodes.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const container = containerRef.current;
    const fullWidth = container.clientWidth;
    const fullHeight = Math.max(container.clientHeight, 300);
    const margin = { top: title ? 36 : 16, right: 20, bottom: 16, left: 20 };

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

    // Build sankey
    const nodeIndex = new Map(nodes.map((n, i) => [n.id, i]));
    const sankeyNodes = nodes.map((n) => ({ ...n }));
    const sankeyLinks = links
      .filter(
        (l) => nodeIndex.has(l.source) && nodeIndex.has(l.target)
      )
      .map((l) => ({
        source: nodeIndex.get(l.source)!,
        target: nodeIndex.get(l.target)!,
        value: l.value,
      }));

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    const sankeyLayout = (d3Sankey as any)()
      .nodeWidth(12)
      .nodePadding(8)
      .extent([
        [margin.left, margin.top],
        [fullWidth - margin.right, fullHeight - margin.bottom],
      ]);

    const { nodes: sNodes, links: sLinks } = sankeyLayout({
      nodes: sankeyNodes,
      links: sankeyLinks,
    }) as { nodes: SNode[]; links: SLink[] };

    const colorScale = d3
      .scaleOrdinal<string>()
      .domain(nodes.map((n) => n.id))
      .range(NODE_COLORS);

    const g = svg.append("g");

    // Links
    g.append("g")
      .selectAll("path")
      .data(sLinks as SLink[])
      .join("path")
      .attr("d", sankeyLinkHorizontal())
      .attr("fill", "none")
      .attr("stroke", (d) => colorScale(d.source.id) + "40")
      .attr("stroke-width", (d) => Math.max(1, d.width || 1))
      .attr("cursor", onLinkClick ? "pointer" : "default")
      .on("mouseover", function (event, d) {
        d3.select(this).attr("stroke", colorScale(d.source.id) + "80");
        const [mx, my] = d3.pointer(event, container);
        setTooltip({
          x: mx,
          y: my,
          source: d.source.label,
          target: d.target.label,
          value: d.value,
        });
      })
      .on("mouseout", function (_, d) {
        d3.select(this).attr("stroke", colorScale(d.source.id) + "40");
        setTooltip(null);
      })
      .on("click", (_, d) =>
        onLinkClick?.(d.source.id, d.target.id)
      );

    // Nodes
    g.append("g")
      .selectAll("rect")
      .data(sNodes as SNode[])
      .join("rect")
      .attr("x", (d) => d.x0!)
      .attr("y", (d) => d.y0!)
      .attr("width", (d) => d.x1! - d.x0!)
      .attr("height", (d) => Math.max(1, d.y1! - d.y0!))
      .attr("rx", 2)
      .attr("fill", (d) => colorScale(d.id))
      .attr("opacity", 0.8);

    // Node labels
    g.append("g")
      .selectAll("text")
      .data(sNodes as SNode[])
      .join("text")
      .attr("x", (d) => (d.x0! < fullWidth / 2 ? d.x1! + 6 : d.x0! - 6))
      .attr("y", (d) => (d.y0! + d.y1!) / 2)
      .attr("text-anchor", (d) => (d.x0! < fullWidth / 2 ? "start" : "end"))
      .attr("dominant-baseline", "middle")
      .attr("fill", "#94a3b8")
      .attr("font-size", "10px")
      .attr("font-family", "monospace")
      .text((d) =>
        d.label.length > 14 ? d.label.slice(0, 13) + "\u2026" : d.label
      );
  }, [nodes, links, title, onLinkClick]);

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
          <div className="text-[10px] text-slate-400 font-mono">
            {tooltip.source} &rarr; {tooltip.target}
          </div>
          <div className="text-xs text-slate-200 font-mono font-bold mt-0.5">
            {tooltip.value.toLocaleString()}
          </div>
        </div>
      )}
    </div>
  );
}
