"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import PipelineLatencyChart from "./PipelineLatencyChart";
import QueueMonitorPanel from "./QueueMonitorPanel";
import MessageFlowGraph from "./MessageFlowGraph";

// ── Types (mirror route.ts EisenbahnMetrics) ──────────────────────────

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

interface TimeSeriesPoint {
  ts: string;
  value: number;
  metric: string;
}

interface EisenbahnMetrics {
  topics: Record<string, TopicMetrics>;
  workers: Record<string, WorkerMetrics>;
  time_series: TimeSeriesPoint[];
  total_messages: number;
  uptime_secs: number;
  stale?: boolean;
  fetched_at: string;
}

// ── Helpers ───────────────────────────────────────────────────────────

function formatUptime(secs: number): string {
  if (secs < 60) return `${secs}s`;
  if (secs < 3600) return `${Math.floor(secs / 60)}m ${secs % 60}s`;
  const h = Math.floor(secs / 3600);
  const m = Math.floor((secs % 3600) / 60);
  return `${h}h ${m}m`;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(1)} GB`;
}

// ── StatCard (matches existing pattern) ───────────────────────────────

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

// ── SectionHeader ─────────────────────────────────────────────────────

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

// ── Worker Card ───────────────────────────────────────────────────────

function WorkerCard({
  name,
  worker,
}: {
  name: string;
  worker: WorkerMetrics;
}) {
  const isOnline = worker.status === "online";
  const accent = isOnline ? "#06d6a0" : "#ff4757";

  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden group transition-all"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accent}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accent}60, transparent)`,
        }}
      />
      {/* Header: name + status dot */}
      <div className="flex items-center justify-between mb-3">
        <span className="text-sm font-bold font-mono tracking-wide text-slate-200 truncate">
          {name}
        </span>
        <div className="flex items-center gap-1.5">
          <div
            className={`w-2 h-2 rounded-full ${isOnline ? "animate-pulse" : ""}`}
            style={{
              background: accent,
              boxShadow: isOnline ? `0 0 6px ${accent}80` : "none",
            }}
          />
          <span
            className="text-[10px] font-bold uppercase tracking-wider"
            style={{ color: accent }}
          >
            {worker.status}
          </span>
        </div>
      </div>

      {/* Metrics grid */}
      <div className="grid grid-cols-3 gap-3">
        {/* CPU */}
        <div>
          <div className="text-[9px] text-slate-600 uppercase tracking-wider mb-1">
            CPU
          </div>
          <div className="text-sm font-mono font-bold" style={{ color: "#00f0ff" }}>
            {worker.cpu_pct.toFixed(1)}%
          </div>
          <div className="mt-1.5 h-1 rounded-full overflow-hidden" style={{ background: "rgba(0, 240, 255, 0.1)" }}>
            <div
              className="h-full rounded-full transition-all duration-500"
              style={{
                width: `${Math.min(worker.cpu_pct, 100)}%`,
                background: worker.cpu_pct > 80 ? "#ff4757" : "#00f0ff",
              }}
            />
          </div>
        </div>

        {/* Memory */}
        <div>
          <div className="text-[9px] text-slate-600 uppercase tracking-wider mb-1">
            Memory
          </div>
          <div className="text-sm font-mono font-bold" style={{ color: "#a855f7" }}>
            {formatBytes(worker.mem_bytes)}
          </div>
        </div>

        {/* Last seen */}
        <div>
          <div className="text-[9px] text-slate-600 uppercase tracking-wider mb-1">
            Last Seen
          </div>
          <div className="text-sm font-mono font-bold" style={{ color: "#ffe600" }}>
            {worker.last_seen_secs_ago === 0
              ? "now"
              : `${worker.last_seen_secs_ago}s ago`}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Main Page ─────────────────────────────────────────────────────────

export default function EisenbahnPage() {
  const [metrics, setMetrics] = useState<EisenbahnMetrics | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [topicFilter, setTopicFilter] = useState<Set<string>>(new Set());

  // Stable topic filter toggle
  const toggleTopic = useCallback((topic: string) => {
    setTopicFilter((prev) => {
      const next = new Set(prev);
      if (next.has(topic)) next.delete(topic);
      else next.add(topic);
      return next;
    });
  }, []);

  const fetchMetrics = useCallback(async () => {
    try {
      const res = await fetch("/api/eisenbahn/metrics");
      if (!res.ok) {
        const body = await res.json().catch(() => ({}));
        throw new Error(body.error || `HTTP ${res.status}`);
      }
      const data: EisenbahnMetrics = await res.json();
      setMetrics(data);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, []);

  // Auto-refresh at 1s interval using refreshKey pattern
  useEffect(() => {
    fetchMetrics();
    const interval = setInterval(() => {
      setRefreshKey((k) => k + 1);
    }, 1000);
    return () => clearInterval(interval);
  }, [fetchMetrics]);

  useEffect(() => {
    if (refreshKey > 0) {
      fetchMetrics();
    }
  }, [refreshKey, fetchMetrics]);

  // Derived data
  const workerEntries = metrics ? Object.entries(metrics.workers) : [];
  const topicEntries = metrics ? Object.entries(metrics.topics) : [];
  const onlineCount = workerEntries.filter(([, w]) => w.status === "online").length;

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(244, 114, 182, 0.08)",
          background:
            "linear-gradient(180deg, rgba(244, 114, 182, 0.02) 0%, transparent 100%)",
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
            style={{ background: "rgba(244, 114, 182, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#f472b6" }}
          >
            Eisenbahn
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            Message Bus
          </span>
        </div>
        <div className="flex items-center gap-4">
          {metrics?.stale && (
            <span className="text-[10px] font-bold uppercase tracking-wider px-2 py-0.5 rounded" style={{
              color: "#ffe600",
              background: "rgba(255, 230, 0, 0.1)",
              border: "1px solid rgba(255, 230, 0, 0.2)",
            }}>
              Stale
            </span>
          )}
          <div className="flex items-center gap-2">
            <div
              className={`w-2 h-2 rounded-full ${
                onlineCount > 0
                  ? "bg-green-400 animate-pulse"
                  : metrics
                    ? "bg-red-400"
                    : "bg-slate-600"
              }`}
            />
            <span className="text-slate-500 text-xs font-mono">
              {onlineCount > 0
                ? `${onlineCount} worker${onlineCount > 1 ? "s" : ""} online`
                : metrics
                  ? "no workers"
                  : "connecting"}
            </span>
          </div>
        </div>
      </header>

      {/* Body */}
      <div className="flex-1 overflow-y-auto px-6 py-6">
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
                Connecting to eisenbahn broker...
              </span>
            </div>
          )}

          {/* Stats cards */}
          {metrics && (
            <>
              <div className="grid grid-cols-2 md:grid-cols-4 gap-3">
                <StatCard
                  label="Total Messages"
                  value={metrics.total_messages}
                  accent="#f472b6"
                />
                <StatCard
                  label="Uptime"
                  value={formatUptime(metrics.uptime_secs)}
                  accent="#06d6a0"
                />
                <StatCard
                  label="Workers"
                  value={`${onlineCount} / ${workerEntries.length}`}
                  accent={onlineCount > 0 ? "#00f0ff" : "#ff4757"}
                />
                <StatCard
                  label="Topics"
                  value={topicEntries.length}
                  accent="#a855f7"
                />
              </div>

              {/* Worker Grid */}
              <section>
                <SectionHeader
                  title="Worker Grid"
                  subtitle={`${workerEntries.length} registered`}
                />
                {workerEntries.length === 0 ? (
                  <div
                    className="rounded-xl p-8 flex items-center justify-center"
                    style={{
                      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                      border: "1px solid rgba(100, 116, 139, 0.1)",
                    }}
                  >
                    <span className="text-slate-600 text-sm font-mono">
                      No workers registered
                    </span>
                  </div>
                ) : (
                  <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-3">
                    {workerEntries.map(([name, worker]) => (
                      <WorkerCard key={name} name={name} worker={worker} />
                    ))}
                  </div>
                )}
              </section>

              {/* Queue Monitor Panel — replaces basic Topics table */}
              {topicEntries.length > 0 && (
                <section>
                  <SectionHeader
                    title="Queue Monitor"
                    subtitle={`${topicEntries.length} topics · depth · throughput · lag`}
                  />
                  <QueueMonitorPanel
                    topics={metrics.topics}
                    timeSeries={metrics.time_series}
                  />
                </section>
              )}

              {/* Message Flow — D3 force-directed graph */}
              <section>
                <SectionHeader
                  title="Message Flow"
                  subtitle={`${Object.keys(metrics.workers).length} workers · ${Object.keys(metrics.topics).length} topics`}
                />
                {/* Topic filter chips */}
                {topicEntries.length > 0 && (
                  <div className="flex flex-wrap gap-1.5 mb-3">
                    {topicEntries.map(([topic]) => {
                      const active = topicFilter.size === 0 || topicFilter.has(topic);
                      return (
                        <button
                          key={topic}
                          onClick={() => toggleTopic(topic)}
                          className="px-2 py-0.5 rounded text-[10px] font-mono transition-all"
                          style={{
                            background: active
                              ? "rgba(244, 114, 182, 0.15)"
                              : "rgba(100, 116, 139, 0.05)",
                            color: active ? "#f472b6" : "#475569",
                            border: `1px solid ${active ? "rgba(244, 114, 182, 0.3)" : "rgba(100, 116, 139, 0.1)"}`,
                          }}
                        >
                          {topic}
                        </button>
                      );
                    })}
                    {topicFilter.size > 0 && (
                      <button
                        onClick={() => setTopicFilter(new Set())}
                        className="px-2 py-0.5 rounded text-[10px] font-mono text-slate-500 hover:text-slate-300 transition-colors"
                        style={{
                          border: "1px solid rgba(100, 116, 139, 0.1)",
                        }}
                      >
                        Clear
                      </button>
                    )}
                  </div>
                )}
                <div
                  className="rounded-xl overflow-hidden"
                  style={{
                    background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                    border: "1px solid rgba(244, 114, 182, 0.1)",
                    boxShadow: "0 0 30px rgba(244, 114, 182, 0.03)",
                    height: 420,
                  }}
                >
                  <MessageFlowGraph
                    metrics={metrics}
                    topicFilter={topicFilter}
                  />
                </div>
              </section>

              {/* Pipeline Latency Breakdown */}
              <section>
                <SectionHeader
                  title="Pipeline Latency"
                  subtitle="ingest → compute → graph"
                />
                <div
                  className="rounded-xl p-4"
                  style={{
                    background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                    border: "1px solid rgba(244, 114, 182, 0.1)",
                    boxShadow: "0 0 30px rgba(244, 114, 182, 0.03)",
                  }}
                >
                  <PipelineLatencyChart refreshKey={refreshKey} />
                </div>
              </section>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
