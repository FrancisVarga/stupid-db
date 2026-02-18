"use client";

import { useEffect, useRef, useState, useCallback, useMemo } from "react";
import * as d3 from "d3";

// ── Types ───────────────────────────────────────────────────────────

interface TopicMetrics {
  count: number;
  rate: number;
}

interface WorkerMetrics {
  status: string;
  cpu_pct: number;
  mem_bytes: number;
  last_seen_secs_ago: number;
}

interface EisenbahnMetrics {
  topics: Record<string, TopicMetrics>;
  workers: Record<string, WorkerMetrics>;
  total_messages: number;
  uptime_secs: number;
}

// D3 simulation node/link types
interface FlowNode extends d3.SimulationNodeDatum {
  id: string;
  type: "worker" | "topic";
  // Worker-specific
  status?: string;
  cpu_pct?: number;
  mem_bytes?: number;
  last_seen_secs_ago?: number;
  // Topic-specific
  count?: number;
  rate?: number;
}

interface FlowLink extends d3.SimulationLinkDatum<FlowNode> {
  id: string;
  topic: string;
  rate: number;
}

// Detail panel payload
type DetailSelection =
  | { type: "worker"; name: string; worker: WorkerMetrics }
  | { type: "topic"; name: string; topic: TopicMetrics; connectedWorkers: string[] }
  | null;

interface Props {
  metrics: EisenbahnMetrics;
  topicFilter: Set<string>;
}

// ── Color helpers ───────────────────────────────────────────────────

function healthColor(status: string): string {
  switch (status) {
    case "online":
      return "#06d6a0";
    case "degraded":
      return "#ffe600";
    default:
      return "#ff4757";
  }
}

const TOPIC_COLOR = "#f472b6";
const BG_DARK = "rgba(6, 8, 13, 0.92)";

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024)
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// ── Component ───────────────────────────────────────────────────────

export default function MessageFlowGraph({ metrics, topicFilter }: Props) {
  const svgRef = useRef<SVGSVGElement>(null);
  const simRef = useRef<d3.Simulation<FlowNode, FlowLink> | null>(null);
  const [detail, setDetail] = useState<DetailSelection>(null);

  // Build graph data from metrics, filtered by topicFilter
  const { nodes, links } = useMemo(() => {
    const workerNames = Object.keys(metrics.workers);
    const topicEntries = Object.entries(metrics.topics).filter(
      ([t]) => topicFilter.size === 0 || topicFilter.has(t)
    );

    const nodeList: FlowNode[] = [];
    const linkList: FlowLink[] = [];

    // Worker nodes
    for (const [name, w] of Object.entries(metrics.workers)) {
      nodeList.push({
        id: `w:${name}`,
        type: "worker",
        status: w.status,
        cpu_pct: w.cpu_pct,
        mem_bytes: w.mem_bytes,
        last_seen_secs_ago: w.last_seen_secs_ago,
      });
    }

    // Topic nodes + edges to every worker
    for (const [topic, tm] of topicEntries) {
      nodeList.push({
        id: `t:${topic}`,
        type: "topic",
        count: tm.count,
        rate: tm.rate,
      });
      // Connect topic to every worker (message bus topology)
      for (const wName of workerNames) {
        linkList.push({
          id: `${topic}→${wName}`,
          source: `t:${topic}`,
          target: `w:${wName}`,
          topic,
          rate: tm.rate,
        });
      }
    }

    return { nodes: nodeList, links: linkList };
  }, [metrics, topicFilter]);

  // Click handler for nodes
  const handleNodeClick = useCallback(
    (d: FlowNode) => {
      if (d.type === "worker") {
        const name = d.id.slice(2); // strip "w:"
        const w = metrics.workers[name];
        if (w) setDetail({ type: "worker", name, worker: w });
      } else {
        const name = d.id.slice(2); // strip "t:"
        const t = metrics.topics[name];
        if (t) {
          setDetail({
            type: "topic",
            name,
            topic: t,
            connectedWorkers: Object.keys(metrics.workers),
          });
        }
      }
    },
    [metrics]
  );

  // Click handler for links (edges)
  const handleLinkClick = useCallback(
    (d: FlowLink) => {
      const topicName = d.topic;
      const t = metrics.topics[topicName];
      if (t) {
        setDetail({
          type: "topic",
          name: topicName,
          topic: t,
          connectedWorkers: Object.keys(metrics.workers),
        });
      }
    },
    [metrics]
  );

  // D3 force simulation — create once, update data reactively
  useEffect(() => {
    if (!svgRef.current) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const width = svgRef.current.clientWidth;
    const height = svgRef.current.clientHeight;

    // Defs: glow filter + animated dash pattern
    const defs = svg.append("defs");
    const glow = defs.append("filter").attr("id", "flow-glow");
    glow
      .append("feGaussianBlur")
      .attr("stdDeviation", "4")
      .attr("result", "coloredBlur");
    const merge = glow.append("feMerge");
    merge.append("feMergeNode").attr("in", "coloredBlur");
    merge.append("feMergeNode").attr("in", "SourceGraphic");

    // Arrow marker for directed edges
    defs
      .append("marker")
      .attr("id", "flow-arrow")
      .attr("viewBox", "0 -3 6 6")
      .attr("refX", 20)
      .attr("refY", 0)
      .attr("markerWidth", 6)
      .attr("markerHeight", 6)
      .attr("orient", "auto")
      .append("path")
      .attr("d", "M0,-3L6,0L0,3")
      .attr("fill", `${TOPIC_COLOR}60`);

    const g = svg.append("g");

    // Zoom
    const zoom = d3
      .zoom<SVGSVGElement, unknown>()
      .scaleExtent([0.2, 6])
      .on("zoom", (event) => g.attr("transform", event.transform));
    svg.call(zoom);

    // Link group, node group (ordering: links below nodes)
    const linkGroup = g.append("g").attr("class", "links");
    const nodeGroup = g.append("g").attr("class", "nodes");
    const labelGroup = g.append("g").attr("class", "labels");

    // Simulation
    const simulation = d3
      .forceSimulation<FlowNode>(nodes)
      .force(
        "link",
        d3
          .forceLink<FlowNode, FlowLink>(links)
          .id((d) => d.id)
          .distance(100)
      )
      .force("charge", d3.forceManyBody().strength(-200))
      .force("center", d3.forceCenter(width / 2, height / 2))
      .force("collision", d3.forceCollide().radius(30));

    simRef.current = simulation;

    // Draw links
    const linkSel = linkGroup
      .selectAll<SVGLineElement, FlowLink>("line")
      .data(links, (d) => d.id)
      .join("line")
      .attr("stroke", `${TOPIC_COLOR}40`)
      .attr("stroke-width", (d) => Math.max(1, Math.min(d.rate * 2, 8)))
      .attr("stroke-dasharray", "6 4")
      .attr("marker-end", "url(#flow-arrow)")
      .attr("cursor", "pointer")
      .on("click", (_, d) => handleLinkClick(d));

    // Animate dash offset for flow direction
    function animateLinks() {
      linkSel
        .attr("stroke-dashoffset", function () {
          const current = parseFloat(
            d3.select(this).attr("stroke-dashoffset") || "0"
          );
          return current - 1;
        });
      requestAnimationFrame(animateLinks);
    }
    const animFrame = requestAnimationFrame(animateLinks);

    // Draw nodes
    const nodeSel = nodeGroup
      .selectAll<SVGCircleElement, FlowNode>("circle")
      .data(nodes, (d) => d.id)
      .join("circle")
      .attr("r", (d) => {
        if (d.type === "topic") {
          // Size by rate
          return Math.max(6, Math.min((d.rate ?? 0) * 3 + 6, 18));
        }
        // Worker: size by CPU usage
        return Math.max(10, Math.min((d.cpu_pct ?? 0) / 5 + 10, 22));
      })
      .attr("fill", (d) => {
        if (d.type === "topic") return TOPIC_COLOR;
        return healthColor(d.status ?? "offline");
      })
      .attr("stroke", (d) => {
        if (d.type === "topic") return `${TOPIC_COLOR}80`;
        return `${healthColor(d.status ?? "offline")}80`;
      })
      .attr("stroke-width", 2)
      .attr("filter", "url(#flow-glow)")
      .attr("cursor", "pointer")
      .on("mouseover", function () {
        d3.select(this)
          .transition()
          .duration(150)
          .attr("stroke", "#fff")
          .attr("stroke-width", 3);
      })
      .on("mouseout", function (_, d) {
        const color =
          d.type === "topic"
            ? `${TOPIC_COLOR}80`
            : `${healthColor(d.status ?? "offline")}80`;
        d3.select(this)
          .transition()
          .duration(150)
          .attr("stroke", color)
          .attr("stroke-width", 2);
      })
      .on("click", (_, d) => handleNodeClick(d));

    // Drag behavior
    const drag = d3
      .drag<SVGCircleElement, FlowNode>()
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
    nodeSel.call(drag as any);

    // Labels
    const labelSel = labelGroup
      .selectAll<SVGTextElement, FlowNode>("text")
      .data(nodes, (d) => d.id)
      .join("text")
      .text((d) => d.id.slice(2)) // strip prefix
      .attr("font-size", (d) => (d.type === "worker" ? 11 : 9))
      .attr("fill", (d) =>
        d.type === "worker" ? "#e2e8f0" : `${TOPIC_COLOR}cc`
      )
      .attr("text-anchor", "middle")
      .attr("dy", (d) => {
        const r =
          d.type === "topic"
            ? Math.max(6, Math.min((d.rate ?? 0) * 3 + 6, 18))
            : Math.max(10, Math.min((d.cpu_pct ?? 0) / 5 + 10, 22));
        return r + 14;
      })
      .attr("font-family", "ui-monospace, monospace")
      .attr("pointer-events", "none");

    // Tick
    simulation.on("tick", () => {
      linkSel
        .attr("x1", (d) => (d.source as FlowNode).x!)
        .attr("y1", (d) => (d.source as FlowNode).y!)
        .attr("x2", (d) => (d.target as FlowNode).x!)
        .attr("y2", (d) => (d.target as FlowNode).y!);

      nodeSel.attr("cx", (d) => d.x!).attr("cy", (d) => d.y!);

      labelSel.attr("x", (d) => d.x!).attr("y", (d) => d.y!);
    });

    return () => {
      cancelAnimationFrame(animFrame);
      simulation.stop();
      simRef.current = null;
    };
  }, [nodes, links, handleNodeClick, handleLinkClick]);

  // Pulse effect: briefly brighten links when rate > 0 (activity indicator)
  useEffect(() => {
    if (!svgRef.current) return;
    const svg = d3.select(svgRef.current);
    svg
      .selectAll<SVGLineElement, FlowLink>(".links line")
      .attr("stroke", (d) =>
        d.rate > 0 ? `${TOPIC_COLOR}70` : `${TOPIC_COLOR}20`
      )
      .filter((d) => d.rate > 0)
      .transition()
      .duration(400)
      .attr("stroke", `${TOPIC_COLOR}40`);
  }, [metrics]);

  return (
    <div className="relative w-full h-full">
      <svg ref={svgRef} className="w-full h-full" />

      {/* Detail panel */}
      {detail && (
        <div
          className="absolute top-3 right-3 w-64 rounded-xl p-4 backdrop-blur-md"
          style={{
            background: BG_DARK,
            border: "1px solid rgba(244, 114, 182, 0.15)",
            boxShadow: "0 0 30px rgba(244, 114, 182, 0.05)",
          }}
        >
          {/* Close button */}
          <button
            onClick={() => setDetail(null)}
            className="absolute top-2 right-2 text-slate-500 hover:text-slate-300 text-xs font-mono"
          >
            ✕
          </button>

          {detail.type === "worker" ? (
            <>
              <div className="flex items-center gap-2 mb-3">
                <div
                  className="w-3 h-3 rounded-full"
                  style={{
                    background: healthColor(detail.worker.status),
                    boxShadow: `0 0 6px ${healthColor(detail.worker.status)}80`,
                  }}
                />
                <span className="text-sm font-bold font-mono text-slate-200 tracking-wide">
                  {detail.name}
                </span>
              </div>
              <div className="space-y-2 text-xs font-mono">
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    Status
                  </span>
                  <span style={{ color: healthColor(detail.worker.status) }}>
                    {detail.worker.status}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    CPU
                  </span>
                  <span style={{ color: "#00f0ff" }}>
                    {detail.worker.cpu_pct.toFixed(1)}%
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    Memory
                  </span>
                  <span style={{ color: "#a855f7" }}>
                    {formatBytes(detail.worker.mem_bytes)}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    Last Seen
                  </span>
                  <span style={{ color: "#ffe600" }}>
                    {detail.worker.last_seen_secs_ago === 0
                      ? "now"
                      : `${detail.worker.last_seen_secs_ago}s ago`}
                  </span>
                </div>
              </div>
            </>
          ) : (
            <>
              <div className="flex items-center gap-2 mb-3">
                <div
                  className="w-3 h-3 rounded-full"
                  style={{
                    background: TOPIC_COLOR,
                    boxShadow: `0 0 6px ${TOPIC_COLOR}80`,
                  }}
                />
                <span className="text-sm font-bold font-mono tracking-wide" style={{ color: TOPIC_COLOR }}>
                  {detail.name}
                </span>
              </div>
              <div className="space-y-2 text-xs font-mono">
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    Messages
                  </span>
                  <span style={{ color: TOPIC_COLOR }}>
                    {detail.topic.count.toLocaleString()}
                  </span>
                </div>
                <div className="flex justify-between">
                  <span className="text-slate-500 uppercase tracking-wider">
                    Rate
                  </span>
                  <span style={{ color: "#00f0ff" }}>
                    {detail.topic.rate.toFixed(1)} msg/s
                  </span>
                </div>
                <div className="mt-2 pt-2" style={{ borderTop: "1px solid rgba(244, 114, 182, 0.1)" }}>
                  <span className="text-slate-500 uppercase tracking-wider text-[9px]">
                    Connected Workers
                  </span>
                  <div className="mt-1 flex flex-wrap gap-1">
                    {detail.connectedWorkers.map((w) => (
                      <span
                        key={w}
                        className="px-1.5 py-0.5 rounded text-[10px]"
                        style={{
                          background: "rgba(6, 214, 160, 0.1)",
                          color: "#06d6a0",
                          border: "1px solid rgba(6, 214, 160, 0.2)",
                        }}
                      >
                        {w}
                      </span>
                    ))}
                  </div>
                </div>
              </div>
            </>
          )}
        </div>
      )}

      {/* Legend */}
      <div
        className="absolute bottom-3 left-3 flex items-center gap-4 px-3 py-2 rounded-lg text-[10px] font-mono"
        style={{
          background: BG_DARK,
          border: "1px solid rgba(100, 116, 139, 0.1)",
        }}
      >
        <div className="flex items-center gap-1.5">
          <div className="w-2.5 h-2.5 rounded-full" style={{ background: "#06d6a0" }} />
          <span className="text-slate-400">Healthy</span>
        </div>
        <div className="flex items-center gap-1.5">
          <div className="w-2.5 h-2.5 rounded-full" style={{ background: "#ffe600" }} />
          <span className="text-slate-400">Degraded</span>
        </div>
        <div className="flex items-center gap-1.5">
          <div className="w-2.5 h-2.5 rounded-full" style={{ background: "#ff4757" }} />
          <span className="text-slate-400">Down</span>
        </div>
        <div className="w-px h-3 bg-slate-700" />
        <div className="flex items-center gap-1.5">
          <div className="w-2.5 h-2.5 rounded-full" style={{ background: TOPIC_COLOR }} />
          <span className="text-slate-400">Topic</span>
        </div>
      </div>
    </div>
  );
}
