"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import * as d3 from "d3";
import type { ForceGraphData, ForceNode, ForceLink } from "@/lib/api";

// Neon-inspired entity colors
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

const ENTITY_RADIUS: Record<string, number> = {
  Member: 5,
  Device: 4,
  Platform: 9,
  Currency: 8,
  VipGroup: 8,
  Affiliate: 6,
};

interface SimNode extends d3.SimulationNodeDatum {
  id: string;
  entity_type: string;
  key: string;
}

interface SimLink extends d3.SimulationLinkDatum<SimNode> {
  edge_type: string;
  weight: number;
}

interface Props {
  data: ForceGraphData;
  onNodeClick?: (nodeId: string) => void;
  communityMap?: Map<string, number>;
}

// 12 distinct community colors for Louvain clusters
const COMMUNITY_COLORS = [
  "#00f0ff", "#ff6eb4", "#06d6a0", "#ffe600", "#c084fc",
  "#ff8a00", "#00ff88", "#ff4757", "#2ec4b6", "#9d4edd",
  "#f97316", "#38bdf8",
];

export default function ForceGraph({ data, onNodeClick, communityMap }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const [tooltip, setTooltip] = useState<{
    x: number;
    y: number;
    node: ForceNode;
  } | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  const handleClick = useCallback(
    (nodeId: string) => {
      setSelectedId((prev) => (prev === nodeId ? null : nodeId));
      onNodeClick?.(nodeId);
    },
    [onNodeClick]
  );

  useEffect(() => {
    if (!svgRef.current || !data.nodes.length) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    // Add glow filter
    const defs = svg.append("defs");
    const filter = defs.append("filter").attr("id", "glow");
    filter
      .append("feGaussianBlur")
      .attr("stdDeviation", "3")
      .attr("result", "coloredBlur");
    const feMerge = filter.append("feMerge");
    feMerge.append("feMergeNode").attr("in", "coloredBlur");
    feMerge.append("feMergeNode").attr("in", "SourceGraphic");

    const g = svg.append("g");

    // Zoom
    const zoom = d3
      .zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.1, 8])
      .on("zoom", (event) => g.attr("transform", event.transform));
    svg.call(zoom);

    // Build simulation data
    const nodes: SimNode[] = data.nodes.map((n) => ({ ...n }));
    const nodeMap = new Map(nodes.map((n) => [n.id, n]));

    const links: SimLink[] = data.links
      .filter((l) => nodeMap.has(l.source) && nodeMap.has(l.target))
      .map((l) => ({
        source: l.source,
        target: l.target,
        edge_type: l.edge_type,
        weight: l.weight,
      }));

    const simulation = d3
      .forceSimulation(nodes)
      .force(
        "link",
        d3
          .forceLink<SimNode, SimLink>(links)
          .id((d) => d.id)
          .distance(60)
      )
      .force("charge", d3.forceManyBody().strength(-40))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collision", d3.forceCollide().radius(8));

    // Links — subtle gradient lines
    const link = g
      .append("g")
      .selectAll("line")
      .data(links)
      .join("line")
      .attr("stroke", "rgba(0, 240, 255, 0.15)")
      .attr("stroke-opacity", 0.5)
      .attr("stroke-width", (d) => Math.min(d.weight, 3));

    // Color resolver: community mode or entity-type mode
    const nodeColor = (d: SimNode) => {
      if (communityMap) {
        const cid = communityMap.get(d.id);
        if (cid !== undefined) return COMMUNITY_COLORS[cid % COMMUNITY_COLORS.length];
      }
      return ENTITY_COLORS[d.entity_type] || "#888";
    };

    // Nodes with glow
    const node = g
      .append("g")
      .selectAll("circle")
      .data(nodes)
      .join("circle")
      .attr("r", (d) => ENTITY_RADIUS[d.entity_type] || 5)
      .attr("fill", (d) => nodeColor(d))
      .attr("stroke", (d) => nodeColor(d))
      .attr("stroke-width", 0.5)
      .attr("stroke-opacity", 0.5)
      .attr("filter", "url(#glow)")
      .attr("cursor", "pointer")
      .on("mouseover", function (event, d) {
        const [x, y] = d3.pointer(event, svgRef.current);
        setTooltip({ x, y, node: d });
        d3.select(this)
          .attr("stroke", "#fff")
          .attr("stroke-width", 2)
          .attr("stroke-opacity", 1);
      })
      .on("mouseout", function (_, d) {
        setTooltip(null);
        d3.select(this)
          .attr("stroke", nodeColor(d))
          .attr("stroke-width", 0.5)
          .attr("stroke-opacity", 0.5);
      })
      .on("click", (_, d) => handleClick(d.id));

    // Apply drag behavior
    const dragBehavior = d3
      .drag<SVGCircleElement, SimNode>()
      .on("start", (event, d) => {
        if (!event.active) simulation.alphaTarget(0.3).restart();
        d.fx = d.x;
        d.fy = d.y;
      })
      .on("drag", (event, d) => {
        d.fx = event.x;
        d.fy = event.y;
      })
      .on("end", (event, d) => {
        if (!event.active) simulation.alphaTarget(0);
        d.fx = null;
        d.fy = null;
      });

    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    node.call(dragBehavior as any);

    simulation.on("tick", () => {
      link
        .attr("x1", (d) => (d.source as SimNode).x!)
        .attr("y1", (d) => (d.source as SimNode).y!)
        .attr("x2", (d) => (d.target as SimNode).x!)
        .attr("y2", (d) => (d.target as SimNode).y!);

      node.attr("cx", (d) => d.x!).attr("cy", (d) => d.y!);
    });

    return () => {
      simulation.stop();
    };
  }, [data, handleClick, communityMap]);

  // Highlight selected node's neighbors
  useEffect(() => {
    if (!svgRef.current) return;
    const svg = d3.select(svgRef.current);

    if (!selectedId) {
      svg.selectAll("circle").attr("opacity", 1);
      svg.selectAll("line").attr("opacity", 0.5);
      return;
    }

    const connectedIds = new Set<string>();
    connectedIds.add(selectedId);
    data.links.forEach((l) => {
      const src = typeof l.source === "string" ? l.source : (l.source as ForceNode).id;
      const tgt = typeof l.target === "string" ? l.target : (l.target as ForceNode).id;
      if (src === selectedId) connectedIds.add(tgt);
      if (tgt === selectedId) connectedIds.add(src);
    });

    svg
      .selectAll<SVGCircleElement, SimNode>("circle")
      .attr("opacity", (d) => (connectedIds.has(d.id) ? 1 : 0.08));
    svg
      .selectAll<SVGLineElement, SimLink>("line")
      .attr("opacity", (d) => {
        const src = typeof d.source === "string" ? d.source : (d.source as SimNode).id;
        const tgt = typeof d.target === "string" ? d.target : (d.target as SimNode).id;
        return src === selectedId || tgt === selectedId ? 0.9 : 0.03;
      });
  }, [selectedId, data.links]);

  return (
    <div className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />
      {tooltip && (
        <div
          className="absolute pointer-events-none rounded-lg px-3 py-2 text-sm backdrop-blur-md"
          style={{
            left: tooltip.x + 12,
            top: tooltip.y - 10,
            background: "rgba(6, 8, 13, 0.85)",
            border: `1px solid ${ENTITY_COLORS[tooltip.node.entity_type]}40`,
            boxShadow: `0 0 20px ${ENTITY_COLORS[tooltip.node.entity_type]}15`,
          }}
        >
          <div className="font-semibold text-xs tracking-wider uppercase" style={{ color: ENTITY_COLORS[tooltip.node.entity_type] }}>
            {tooltip.node.entity_type}
          </div>
          <div className="text-slate-300 font-mono text-xs mt-0.5">{tooltip.node.key}</div>
        </div>
      )}
    </div>
  );
}
