"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import Link from "next/link";
import { fetchQueueStatus, WS_URL, type QueueStatus } from "@/lib/api";

// ── Types ────────────────────────────────────────────────────────────────

interface FlowPoint {
  time: Date;
  processed: number;
  failed: number;
}

interface BatchEvent {
  timestamp: Date;
  docs: number;
  graphOps: number;
}

// ── StatCard (matches main page pattern) ─────────────────────────────────

function StatCard({
  label,
  value,
  accent = "#00f0ff",
}: {
  label: string;
  value: string | number;
  accent?: string;
}) {
  return (
    <div className="stat-card rounded-xl p-4 relative overflow-hidden">
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accent}40, transparent)`,
        }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">
        {label}
      </div>
      <div
        className="text-2xl font-bold font-mono mt-1"
        style={{ color: accent }}
      >
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}

// ── Relative time helper ─────────────────────────────────────────────────

function relativeTime(epochMs: number | undefined | null): string {
  if (!epochMs) return "never";
  const diff = Date.now() - epochMs;
  if (diff < 1000) return "just now";
  if (diff < 60_000) return `${Math.floor(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.floor(diff / 60_000)}m ago`;
  return `${Math.floor(diff / 3_600_000)}h ago`;
}

// ── Message Flow Chart (D3 area chart) ───────────────────────────────────

function MessageFlowChart({ data }: { data: FlowPoint[] }) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || data.length < 2) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const width = containerRef.current.clientWidth;
    const height = 220;
    const margin = { top: 20, right: 20, bottom: 30, left: 50 };
    const innerW = width - margin.left - margin.right;
    const innerH = height - margin.top - margin.bottom;

    svg.attr("width", width).attr("height", height);

    const x = d3
      .scaleTime()
      .domain(d3.extent(data, (d) => d.time) as [Date, Date])
      .range([0, innerW]);

    const maxY = d3.max(data, (d) => Math.max(d.processed, d.failed)) || 1;
    const y = d3.scaleLinear().domain([0, maxY * 1.1]).range([innerH, 0]);

    const g = svg
      .append("g")
      .attr("transform", `translate(${margin.left},${margin.top})`);

    // Grid lines
    g.append("g")
      .attr("class", "grid")
      .call(
        d3
          .axisLeft(y)
          .ticks(5)
          .tickSize(-innerW)
          .tickFormat(() => "")
      )
      .selectAll("line")
      .attr("stroke", "rgba(100, 116, 139, 0.1)");
    g.selectAll(".grid .domain").remove();

    // Area + line for processed
    const processedArea = d3
      .area<FlowPoint>()
      .x((d) => x(d.time))
      .y0(innerH)
      .y1((d) => y(d.processed))
      .curve(d3.curveMonotoneX);

    const processedLine = d3
      .line<FlowPoint>()
      .x((d) => x(d.time))
      .y((d) => y(d.processed))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("d", processedArea)
      .attr("fill", "rgba(0, 240, 255, 0.08)");

    g.append("path")
      .datum(data)
      .attr("d", processedLine)
      .attr("fill", "none")
      .attr("stroke", "#00f0ff")
      .attr("stroke-width", 1.5);

    // Area + line for failed
    const failedArea = d3
      .area<FlowPoint>()
      .x((d) => x(d.time))
      .y0(innerH)
      .y1((d) => y(d.failed))
      .curve(d3.curveMonotoneX);

    const failedLine = d3
      .line<FlowPoint>()
      .x((d) => x(d.time))
      .y((d) => y(d.failed))
      .curve(d3.curveMonotoneX);

    g.append("path")
      .datum(data)
      .attr("d", failedArea)
      .attr("fill", "rgba(255, 71, 87, 0.08)");

    g.append("path")
      .datum(data)
      .attr("d", failedLine)
      .attr("fill", "none")
      .attr("stroke", "#ff4757")
      .attr("stroke-width", 1.5);

    // X axis
    g.append("g")
      .attr("transform", `translate(0,${innerH})`)
      .call(
        d3
          .axisBottom(x)
          .ticks(6)
          .tickFormat((d) => d3.timeFormat("%H:%M:%S")(d as Date))
      )
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace");

    g.selectAll(".domain").attr("stroke", "rgba(100, 116, 139, 0.2)");
    g.selectAll(".tick line").attr("stroke", "rgba(100, 116, 139, 0.15)");

    // Y axis
    g.append("g")
      .call(d3.axisLeft(y).ticks(5))
      .selectAll("text")
      .attr("fill", "#475569")
      .attr("font-size", "9px")
      .attr("font-family", "monospace");

    // Legend
    const legend = svg
      .append("g")
      .attr("transform", `translate(${margin.left + 8}, 12)`);

    legend
      .append("rect")
      .attr("width", 10)
      .attr("height", 3)
      .attr("rx", 1)
      .attr("fill", "#00f0ff");
    legend
      .append("text")
      .attr("x", 14)
      .attr("y", 3)
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text("Processed");

    legend
      .append("rect")
      .attr("x", 85)
      .attr("width", 10)
      .attr("height", 3)
      .attr("rx", 1)
      .attr("fill", "#ff4757");
    legend
      .append("text")
      .attr("x", 99)
      .attr("y", 3)
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("font-family", "monospace")
      .text("Failed");
  }, [data]);

  return (
    <div ref={containerRef} className="w-full">
      {data.length < 2 ? (
        <div className="flex items-center justify-center h-[220px]">
          <span className="text-slate-600 text-sm font-mono animate-pulse">
            Collecting data points...
          </span>
        </div>
      ) : (
        <svg ref={svgRef} className="w-full" />
      )}
    </div>
  );
}

// ── Main Page ────────────────────────────────────────────────────────────

export default function QueueMonitorPage() {
  const [status, setStatus] = useState<QueueStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flowData, setFlowData] = useState<FlowPoint[]>([]);
  const [batchEvents, setBatchEvents] = useState<BatchEvent[]>([]);
  const prevStatusRef = useRef<QueueStatus | null>(null);

  // Poll queue status every 5 seconds
  useEffect(() => {
    let cancelled = false;

    const poll = async () => {
      try {
        const s = await fetchQueueStatus();
        if (cancelled) return;
        queueMicrotask(() => {
          setStatus(s);
          setLoading(false);
          setError(null);
        });
      } catch (e) {
        if (cancelled) return;
        queueMicrotask(() => {
          setError((e as Error).message);
          setLoading(false);
        });
      }
    };

    poll();
    const interval = setInterval(poll, 5000);
    return () => {
      cancelled = true;
      clearInterval(interval);
    };
  }, []);

  // Accumulate flow data points from status polls (keep last 60 = 5 min)
  useEffect(() => {
    if (!status) return;

    const prev = prevStatusRef.current;
    const processed = status.messages_processed ?? 0;
    const failed = status.messages_failed ?? 0;

    // Only add a point if values changed or it's the first one
    if (!prev || prev.messages_processed !== processed || prev.messages_failed !== failed) {
      setFlowData((fd) => {
        const next = [
          ...fd,
          { time: new Date(), processed, failed },
        ];
        return next.length > 60 ? next.slice(-60) : next;
      });
    }
  }, [status]);

  // Track previous status for diff detection — must be in useEffect, not render
  useEffect(() => {
    prevStatusRef.current = status;
  }, [status]);

  // WebSocket for batch events
  useEffect(() => {
    let ws: WebSocket | null = null;
    let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

    const connect = () => {
      try {
        ws = new WebSocket(WS_URL);

        ws.onmessage = (evt) => {
          try {
            const msg = JSON.parse(evt.data);
            if (msg.type === "queue_batch") {
              const event: BatchEvent = {
                timestamp: new Date(),
                docs: msg.docs ?? 0,
                graphOps: msg.graph_ops ?? 0,
              };
              setBatchEvents((prev) => {
                const next = [event, ...prev];
                return next.length > 50 ? next.slice(0, 50) : next;
              });
            }
          } catch {
            // ignore non-JSON messages
          }
        };

        ws.onclose = () => {
          reconnectTimer = setTimeout(connect, 3000);
        };

        ws.onerror = () => {
          ws?.close();
        };
      } catch {
        reconnectTimer = setTimeout(connect, 3000);
      }
    };

    connect();

    return () => {
      ws?.close();
      if (reconnectTimer) clearTimeout(reconnectTimer);
    };
  }, []);

  // Derive connection status for the dot indicator
  const isConnected = status?.enabled && status?.connected;

  return (
    <div
      className="min-h-screen"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
      }}
    >
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div
            className="w-[1px] h-4"
            style={{ background: "rgba(0, 240, 255, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            Queue Monitor
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <div
            className={`w-2 h-2 rounded-full ${
              isConnected
                ? "bg-green-400 animate-pulse"
                : status?.enabled
                  ? "bg-red-400"
                  : "bg-slate-600"
            }`}
          />
          <span className="text-slate-500 text-xs font-mono">
            {isConnected
              ? "connected"
              : status?.enabled
                ? "disconnected"
                : "disabled"}
          </span>
        </div>
      </header>

      <div className="px-6 py-6 max-w-[1400px] mx-auto space-y-6">
        {/* Error state */}
        {error && (
          <div
            className="rounded-xl p-4"
            style={{
              background: "rgba(255, 71, 87, 0.05)",
              border: "1px solid rgba(255, 71, 87, 0.2)",
            }}
          >
            <span className="text-red-400 text-sm font-mono">{error}</span>
          </div>
        )}

        {/* Loading state */}
        {loading && !error && (
          <div className="flex items-center justify-center py-20">
            <span className="text-slate-600 text-sm font-mono animate-pulse">
              Connecting to queue...
            </span>
          </div>
        )}

        {/* Status Cards */}
        {status && (
          <>
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-3">
              <StatCard
                label="Status"
                value={status.enabled ? "ENABLED" : "DISABLED"}
                accent={status.enabled ? "#00ff88" : "#ff4757"}
              />
              <StatCard
                label="Connection"
                value={status.connected ? "UP" : "DOWN"}
                accent={status.connected ? "#00ff88" : "#ff4757"}
              />
              <StatCard
                label="Received"
                value={status.messages_received ?? 0}
                accent="#00f0ff"
              />
              <StatCard
                label="Processed"
                value={status.messages_processed ?? 0}
                accent="#a855f7"
              />
              <StatCard
                label="Failed"
                value={status.messages_failed ?? 0}
                accent={
                  (status.messages_failed ?? 0) > 0 ? "#ff4757" : "#64748b"
                }
              />
              <StatCard
                label="Batches"
                value={status.batches_processed ?? 0}
                accent="#f472b6"
              />
              <StatCard
                label="Avg Latency"
                value={
                  status.avg_batch_latency_ms != null
                    ? `${status.avg_batch_latency_ms.toFixed(1)}ms`
                    : "—"
                }
                accent="#ffe600"
              />
              <StatCard
                label="Last Poll"
                value={relativeTime(status.last_poll_epoch_ms)}
                accent="#06d6a0"
              />
            </div>

            {/* Message Flow Chart */}
            <section>
              <SectionHeader title="Message Flow" subtitle="5 min window" />
              <div
                className="rounded-xl p-4"
                style={{
                  background:
                    "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                  border: "1px solid rgba(0, 240, 255, 0.1)",
                  boxShadow: "0 0 30px rgba(0, 240, 255, 0.03)",
                }}
              >
                <MessageFlowChart data={flowData} />
              </div>
            </section>

            {/* Recent Batches Table */}
            <section>
              <SectionHeader
                title="Recent Batches"
                subtitle={`${batchEvents.length} events`}
              />
              <div
                className="rounded-xl overflow-hidden"
                style={{
                  background:
                    "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                  border: "1px solid rgba(0, 240, 255, 0.1)",
                  boxShadow: "0 0 30px rgba(0, 240, 255, 0.03)",
                }}
              >
                {batchEvents.length === 0 ? (
                  <div className="flex items-center justify-center py-12">
                    <span className="text-slate-600 text-sm font-mono">
                      Waiting for batch events via WebSocket...
                    </span>
                  </div>
                ) : (
                  <div className="max-h-[400px] overflow-y-auto">
                    <table className="w-full">
                      <thead>
                        <tr
                          style={{
                            borderBottom:
                              "1px solid rgba(0, 240, 255, 0.08)",
                          }}
                        >
                          <th className="text-left text-[10px] text-slate-500 uppercase tracking-widest font-medium px-4 py-3">
                            Timestamp
                          </th>
                          <th className="text-right text-[10px] text-slate-500 uppercase tracking-widest font-medium px-4 py-3">
                            Docs
                          </th>
                          <th className="text-right text-[10px] text-slate-500 uppercase tracking-widest font-medium px-4 py-3">
                            Graph Ops
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {batchEvents.map((evt, i) => (
                          <tr
                            key={i}
                            className="transition-colors"
                            style={{
                              borderBottom:
                                "1px solid rgba(30, 41, 59, 0.4)",
                            }}
                            onMouseEnter={(e) => {
                              (
                                e.currentTarget as HTMLElement
                              ).style.background =
                                "rgba(0, 240, 255, 0.03)";
                            }}
                            onMouseLeave={(e) => {
                              (
                                e.currentTarget as HTMLElement
                              ).style.background = "transparent";
                            }}
                          >
                            <td className="px-4 py-2.5 text-slate-400 text-xs font-mono">
                              {evt.timestamp.toLocaleTimeString()}
                            </td>
                            <td
                              className="px-4 py-2.5 text-right text-xs font-mono font-bold"
                              style={{ color: "#00f0ff" }}
                            >
                              {evt.docs.toLocaleString()}
                            </td>
                            <td
                              className="px-4 py-2.5 text-right text-xs font-mono font-bold"
                              style={{ color: "#a855f7" }}
                            >
                              {evt.graphOps.toLocaleString()}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            </section>
          </>
        )}
      </div>
    </div>
  );
}

// ── Section Header ───────────────────────────────────────────────────────

function SectionHeader({
  title,
  subtitle,
}: {
  title: string;
  subtitle: string;
}) {
  return (
    <div className="flex items-baseline gap-3 mb-3">
      <h2 className="text-sm font-bold tracking-wider text-slate-300">
        {title}
      </h2>
      <span className="text-[10px] text-slate-600 font-mono tracking-wider uppercase">
        {subtitle}
      </span>
    </div>
  );
}
