"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import * as d3 from "d3";
import Link from "next/link";
import {
  fetchQueueStatus,
  fetchQueueConnections,
  deleteQueueConnectionApi,
  WS_URL,
  type QueueStatus,
  type QueueMetricsEntry,
  type QueueConnectionSafe,
} from "@/lib/api";
import QueueSidebar from "@/components/db/QueueSidebar";
import QueueConnectionForm from "@/components/db/QueueConnectionForm";

// ── Types ────────────────────────────────────────────────────────────────

interface FlowPoint {
  time: Date;
  processed: number;
  failed: number;
}

interface MessageDetail {
  event_type: string;
  id: string;
  timestamp: string;
  fields: Record<string, unknown>;
}

interface BatchEvent {
  timestamp: Date;
  docs: number;
  graphOps: number;
  messages: MessageDetail[];
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

// ── Event type color helper ──────────────────────────────────────────────

const EVENT_COLORS: Record<string, string> = {
  Login: "#00ff88",
  GameOpened: "#a855f7",
  PopupModule: "#f472b6",
  Unknown: "#64748b",
};

function eventColor(eventType: string): string {
  return EVENT_COLORS[eventType] || "#00f0ff";
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

// ── Batch Detail Drawer ──────────────────────────────────────────────────

function BatchDrawer({
  batch,
  onClose,
}: {
  batch: BatchEvent;
  onClose: () => void;
}) {
  // Close on Escape key
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40"
        style={{ background: "rgba(0, 0, 0, 0.5)" }}
        onClick={onClose}
      />

      {/* Drawer panel */}
      <div
        className="fixed top-0 right-0 h-full z-50 overflow-y-auto"
        style={{
          width: "min(480px, 90vw)",
          background: "linear-gradient(180deg, #0c1018 0%, #111827 100%)",
          borderLeft: "1px solid rgba(0, 240, 255, 0.15)",
          boxShadow: "-8px 0 40px rgba(0, 0, 0, 0.5)",
        }}
      >
        {/* Header */}
        <div
          className="sticky top-0 z-10 px-5 py-4 flex items-center justify-between"
          style={{
            background: "rgba(12, 16, 24, 0.95)",
            backdropFilter: "blur(12px)",
            borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <div>
            <div className="text-sm font-bold tracking-wider" style={{ color: "#00f0ff" }}>
              Batch Details
            </div>
            <div className="text-[10px] text-slate-500 font-mono mt-0.5">
              {batch.timestamp.toLocaleTimeString()} &middot; {batch.docs} docs &middot; {batch.graphOps} graph ops
            </div>
          </div>
          <button
            onClick={onClose}
            className="text-slate-500 hover:text-slate-300 transition-colors p-1"
          >
            <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2">
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Messages */}
        <div className="px-5 py-4 space-y-3">
          {batch.messages.length === 0 ? (
            <div className="text-slate-600 text-sm font-mono text-center py-8">
              No message details available
            </div>
          ) : (
            batch.messages.map((msg, i) => (
              <div
                key={msg.id || i}
                className="rounded-lg p-3"
                style={{
                  background: "rgba(30, 41, 59, 0.3)",
                  border: "1px solid rgba(100, 116, 139, 0.12)",
                }}
              >
                {/* Event type badge + timestamp */}
                <div className="flex items-center justify-between mb-2">
                  <span
                    className="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded"
                    style={{
                      color: eventColor(msg.event_type),
                      background: `${eventColor(msg.event_type)}15`,
                      border: `1px solid ${eventColor(msg.event_type)}30`,
                    }}
                  >
                    {msg.event_type}
                  </span>
                  <span className="text-[10px] text-slate-600 font-mono">
                    {new Date(msg.timestamp).toLocaleTimeString()}
                  </span>
                </div>

                {/* ID */}
                <div className="text-[9px] text-slate-600 font-mono mb-2 truncate">
                  {msg.id}
                </div>

                {/* Fields grid */}
                {Object.keys(msg.fields).length > 0 && (
                  <div className="grid grid-cols-2 gap-x-3 gap-y-1">
                    {Object.entries(msg.fields).map(([key, value]) => (
                      <div key={key} className="flex flex-col min-w-0">
                        <span className="text-[9px] text-slate-600 uppercase tracking-wider truncate">
                          {key}
                        </span>
                        <span className="text-xs text-slate-300 font-mono truncate">
                          {value === null ? "null" : String(value)}
                        </span>
                      </div>
                    ))}
                  </div>
                )}
              </div>
            ))
          )}
        </div>
      </div>
    </>
  );
}

// ── Main Page ────────────────────────────────────────────────────────────

export default function QueueMonitorPage() {
  const [status, setStatus] = useState<QueueStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [flowData, setFlowData] = useState<FlowPoint[]>([]);
  const [batchEvents, setBatchEvents] = useState<BatchEvent[]>([]);
  const [selectedBatch, setSelectedBatch] = useState<BatchEvent | null>(null);
  const prevProcessedRef = useRef<number>(0);

  // Connection management state
  const [queues, setQueues] = useState<QueueConnectionSafe[]>([]);
  const [queuesLoading, setQueuesLoading] = useState(true);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConnection, setEditingConnection] = useState<QueueConnectionSafe | null>(null);
  const [sidebarKey, setSidebarKey] = useState(0);
  const [selectedQueue, setSelectedQueue] = useState<string | null>(null);

  const closeDrawer = useCallback(() => setSelectedBatch(null), []);

  const loadQueues = useCallback(() => {
    setQueuesLoading(true);
    fetchQueueConnections()
      .then((qs) => {
        setQueues(qs);
        setQueuesLoading(false);
        setSidebarKey((k) => k + 1);
      })
      .catch(() => {
        setQueuesLoading(false);
      });
  }, []);

  useEffect(() => {
    loadQueues();
  }, [loadQueues]);

  const handleDeleteQueue = async (id: string, name: string) => {
    if (!confirm(`Remove queue connection "${name}"?`)) return;
    try {
      await deleteQueueConnectionApi(id);
      loadQueues();
    } catch (e) {
      setError((e as Error).message);
    }
  };

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

  // Aggregate metrics across all queues (or selected queue).
  const activeMetrics: QueueMetricsEntry | null = (() => {
    if (!status?.queues) return null;
    const entries = Object.entries(status.queues);
    if (entries.length === 0) return null;

    if (selectedQueue && status.queues[selectedQueue]) {
      return status.queues[selectedQueue];
    }

    // Aggregate across all queues.
    const agg: QueueMetricsEntry = {
      enabled: true,
      connected: false,
      messages_received: 0,
      messages_processed: 0,
      messages_failed: 0,
      batches_processed: 0,
      avg_batch_latency_ms: 0,
      last_poll_epoch_ms: 0,
    };
    let totalLatency = 0;
    for (const [, m] of entries) {
      agg.connected = agg.connected || m.connected;
      agg.messages_received += m.messages_received;
      agg.messages_processed += m.messages_processed;
      agg.messages_failed += m.messages_failed;
      agg.batches_processed += m.batches_processed;
      totalLatency += m.avg_batch_latency_ms * m.batches_processed;
      agg.last_poll_epoch_ms = Math.max(agg.last_poll_epoch_ms, m.last_poll_epoch_ms);
    }
    agg.avg_batch_latency_ms = agg.batches_processed > 0 ? totalLatency / agg.batches_processed : 0;
    return agg;
  })();

  // Accumulate flow data points from status polls (keep last 60 = 5 min)
  useEffect(() => {
    if (!activeMetrics) return;

    const processed = activeMetrics.messages_processed;
    const failed = activeMetrics.messages_failed;

    setFlowData((fd) => {
      const last = fd[fd.length - 1];
      if (last && last.processed === processed && last.failed === failed) return fd;
      const next = [
        ...fd,
        { time: new Date(), processed, failed },
      ];
      return next.length > 60 ? next.slice(-60) : next;
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activeMetrics?.messages_processed, activeMetrics?.messages_failed]);

  // Track previous processed count for diff detection.
  useEffect(() => {
    if (activeMetrics) {
      prevProcessedRef.current = activeMetrics.messages_processed;
    }
  });

  // WebSocket for batch events (now with message details)
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
                messages: Array.isArray(msg.messages) ? msg.messages : [],
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
  const isConnected = status?.enabled && activeMetrics?.connected;

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(255, 138, 0, 0.08)",
          background:
            "linear-gradient(180deg, rgba(255, 138, 0, 0.02) 0%, transparent 100%)",
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
            style={{ background: "rgba(255, 138, 0, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#ff8a00" }}
          >
            Queue Manager
          </h1>
        </div>
        <div className="flex items-center gap-4">
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
          <button
            onClick={() => setShowAddForm(true)}
            className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              background: "rgba(255, 138, 0, 0.1)",
              border: "1px solid rgba(255, 138, 0, 0.3)",
              color: "#ff8a00",
            }}
          >
            + Add Queue
          </button>
        </div>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 260 }} className="shrink-0">
          <QueueSidebar refreshKey={sidebarKey} selectedId={selectedQueue} onSelect={setSelectedQueue} />
        </div>

        <div className="flex-1 overflow-y-auto px-6 py-6">
        {/* Add / Edit Queue Form */}
        {(showAddForm || editingConnection) && (
          <div className="mb-6 max-w-[1400px] mx-auto">
            <QueueConnectionForm
              editing={editingConnection ?? undefined}
              onSaved={() => {
                setShowAddForm(false);
                setEditingConnection(null);
                loadQueues();
              }}
              onCancel={() => {
                setShowAddForm(false);
                setEditingConnection(null);
              }}
            />
          </div>
        )}

        {/* Queue connection cards */}
        {!queuesLoading && queues.length > 0 && !showAddForm && !editingConnection && (
          <div className="mb-6 max-w-[1400px] mx-auto">
            <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
              Queue Connections
            </h2>
            <div className="grid grid-cols-2 gap-3">
              {queues.map((q) => (
                <div
                  key={q.id}
                  className="rounded-xl p-4 relative overflow-hidden group"
                  style={{
                    background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                    border: `1px solid ${q.enabled ? `${q.color}20` : "rgba(100, 116, 139, 0.15)"}`,
                  }}
                >
                  <div
                    className="absolute top-0 left-0 w-full h-[1px]"
                    style={{
                      background: `linear-gradient(90deg, transparent, ${q.color}60, transparent)`,
                    }}
                  />
                  <div className="flex items-center justify-between mb-2">
                    <div className="flex items-center gap-2">
                      <span
                        className="w-2 h-2 rounded-full shrink-0"
                        style={{
                          background: q.enabled ? "#06d6a0" : "#64748b",
                          boxShadow: q.enabled ? "0 0 6px rgba(6, 214, 160, 0.5)" : "none",
                        }}
                      />
                      <span className="text-sm font-bold font-mono tracking-wide" style={{ color: q.color }}>
                        {q.name}
                      </span>
                    </div>
                    <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                      <button
                        onClick={() => {
                          setShowAddForm(false);
                          setEditingConnection(q);
                        }}
                        className="text-slate-700 hover:text-purple-400 text-[10px]"
                        title="Edit queue"
                      >
                        Edit
                      </button>
                      <button
                        onClick={() => handleDeleteQueue(q.id, q.name)}
                        className="text-slate-700 hover:text-red-400 text-[10px]"
                        title="Remove queue"
                      >
                        ✕
                      </button>
                    </div>
                  </div>
                  <div className="text-[9px] text-slate-600 font-mono mb-1 truncate">
                    {q.queue_url}
                  </div>
                  <div className="flex items-center gap-3 mt-2">
                    <span className="text-[10px] text-slate-500 font-mono">{q.provider}</span>
                    <span className="text-[10px] text-slate-500 font-mono">{q.region}</span>
                    <span
                      className="text-[10px] font-mono"
                      style={{ color: q.enabled ? "#06d6a0" : "#64748b" }}
                    >
                      {q.enabled ? "ENABLED" : "DISABLED"}
                    </span>
                  </div>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Empty state */}
        {!queuesLoading && queues.length === 0 && !showAddForm && (
          <div className="flex flex-col items-center justify-center py-20 max-w-[1400px] mx-auto">
            <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5" className="mb-4">
              <rect x="2" y="6" width="20" height="12" rx="2" />
              <path d="M12 6V4" />
              <path d="M12 20v-2" />
              <path d="M6 12h12" />
            </svg>
            <p className="text-slate-500 text-sm font-mono mb-2">No queue connections configured</p>
            <p className="text-slate-600 text-xs font-mono mb-4">Add an SQS queue connection to get started</p>
            <button
              onClick={() => setShowAddForm(true)}
              className="px-4 py-2 rounded-lg text-xs font-bold uppercase tracking-wider transition-all hover:opacity-80"
              style={{
                background: "rgba(255, 138, 0, 0.1)",
                border: "1px solid rgba(255, 138, 0, 0.3)",
                color: "#ff8a00",
              }}
            >
              + Add Your First Queue
            </button>
          </div>
        )}

        <div className="max-w-[1400px] mx-auto space-y-6">
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
        {activeMetrics && (
          <>
            <div className="grid grid-cols-2 md:grid-cols-4 lg:grid-cols-8 gap-3">
              <StatCard
                label="Status"
                value={activeMetrics.enabled ? "ENABLED" : "DISABLED"}
                accent={activeMetrics.enabled ? "#00ff88" : "#ff4757"}
              />
              <StatCard
                label="Connection"
                value={activeMetrics.connected ? "UP" : "DOWN"}
                accent={activeMetrics.connected ? "#00ff88" : "#ff4757"}
              />
              <StatCard
                label="Received"
                value={activeMetrics.messages_received}
                accent="#00f0ff"
              />
              <StatCard
                label="Processed"
                value={activeMetrics.messages_processed}
                accent="#a855f7"
              />
              <StatCard
                label="Failed"
                value={activeMetrics.messages_failed}
                accent={
                  activeMetrics.messages_failed > 0 ? "#ff4757" : "#64748b"
                }
              />
              <StatCard
                label="Batches"
                value={activeMetrics.batches_processed}
                accent="#f472b6"
              />
              <StatCard
                label="Avg Latency"
                value={
                  activeMetrics.avg_batch_latency_ms > 0
                    ? `${activeMetrics.avg_batch_latency_ms.toFixed(1)}ms`
                    : "\u2014"
                }
                accent="#ffe600"
              />
              <StatCard
                label="Last Poll"
                value={relativeTime(activeMetrics.last_poll_epoch_ms)}
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
                subtitle={`${batchEvents.length} events \u2014 click a row for details`}
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
                          <th className="text-left text-[10px] text-slate-500 uppercase tracking-widest font-medium px-4 py-3">
                            Events
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
                        {batchEvents.map((evt, i) => {
                          // Collect unique event types for badge display
                          const eventTypes = [
                            ...new Set(evt.messages.map((m) => m.event_type)),
                          ];
                          return (
                            <tr
                              key={i}
                              className="transition-colors cursor-pointer"
                              style={{
                                borderBottom:
                                  "1px solid rgba(30, 41, 59, 0.4)",
                              }}
                              onClick={() => setSelectedBatch(evt)}
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
                              <td className="px-4 py-2.5">
                                <div className="flex gap-1 flex-wrap">
                                  {eventTypes.slice(0, 3).map((et) => (
                                    <span
                                      key={et}
                                      className="text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
                                      style={{
                                        color: eventColor(et),
                                        background: `${eventColor(et)}10`,
                                        border: `1px solid ${eventColor(et)}25`,
                                      }}
                                    >
                                      {et}
                                    </span>
                                  ))}
                                  {eventTypes.length > 3 && (
                                    <span className="text-[9px] text-slate-600 font-mono">
                                      +{eventTypes.length - 3}
                                    </span>
                                  )}
                                </div>
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
                          );
                        })}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>
            </section>
          </>
        )}
        </div>{/* end max-w container */}
        </div>{/* end overflow-y-auto */}
      </div>{/* end flex-1 flex */}

      {/* Side drawer for batch details */}
      {selectedBatch && (
        <BatchDrawer batch={selectedBatch} onClose={closeDrawer} />
      )}
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
