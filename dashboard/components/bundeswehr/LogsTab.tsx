"use client";

import { useEffect, useState } from "react";
import { fetchAgentTelemetry, type TelemetryEvent } from "@/lib/api";

interface LogsTabProps {
  agentName: string;
}

type StatusFilter = "all" | "success" | "error" | "timeout";

const STATUS_COLORS: Record<string, string> = {
  success: "#06d6a0",
  error: "#ff4757",
  timeout: "#fbbf24",
};

const PAGE_SIZE = 25;

export default function LogsTab({ agentName }: LogsTabProps) {
  const [events, setEvents] = useState<TelemetryEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<StatusFilter>("all");
  const [expandedId, setExpandedId] = useState<string | null>(null);
  const [visibleCount, setVisibleCount] = useState(PAGE_SIZE);

  useEffect(() => {
    setLoading(true);
    setError(null);
    fetchAgentTelemetry(agentName, 100)
      .then(setEvents)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to fetch logs"))
      .finally(() => setLoading(false));
  }, [agentName]);

  if (loading) {
    return (
      <div className="space-y-2">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="h-10 rounded-lg animate-pulse"
            style={{ background: "rgba(0, 240, 255, 0.03)" }}
          />
        ))}
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="px-4 py-3 rounded-lg text-xs"
        style={{
          background: "rgba(255, 71, 87, 0.06)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
          color: "#ff4757",
        }}
      >
        {error}
      </div>
    );
  }

  if (events.length === 0) {
    return (
      <div className="py-8 text-center">
        <div className="text-slate-500 text-sm mb-1">No execution logs</div>
        <div className="text-slate-600 text-xs font-mono">
          Agent &quot;{agentName}&quot; has not been executed yet
        </div>
      </div>
    );
  }

  const filtered = filter === "all" ? events : events.filter((e) => e.status === filter);
  const visible = filtered.slice(0, visibleCount);

  return (
    <div>
      {/* Filter bar */}
      <div className="flex items-center gap-2 mb-4">
        {(["all", "success", "error", "timeout"] as StatusFilter[]).map((f) => {
          const count = f === "all" ? events.length : events.filter((e) => e.status === f).length;
          const isActive = filter === f;
          const color = f === "all" ? "#00f0ff" : STATUS_COLORS[f];
          return (
            <button
              key={f}
              onClick={() => { setFilter(f); setVisibleCount(PAGE_SIZE); }}
              className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-[10px] font-bold uppercase tracking-wider transition-opacity hover:opacity-80"
              style={{
                background: isActive ? `${color}20` : "transparent",
                border: `1px solid ${isActive ? `${color}40` : "rgba(100,116,139,0.15)"}`,
                color: isActive ? color : "#64748b",
              }}
            >
              {f}
              <span className="font-mono">{count}</span>
            </button>
          );
        })}
      </div>

      {/* Table header */}
      <div
        className="grid items-center px-4 py-2 text-[9px] font-bold uppercase tracking-[0.15em] text-slate-500"
        style={{ gridTemplateColumns: "140px 1fr 80px 80px 80px" }}
      >
        <span>Timestamp</span>
        <span>Task</span>
        <span className="text-right">Status</span>
        <span className="text-right">Latency</span>
        <span className="text-right">Tokens</span>
      </div>

      {/* Rows */}
      <div className="space-y-1">
        {visible.map((ev) => {
          const color = STATUS_COLORS[ev.status] || "#64748b";
          const isExpanded = expandedId === ev.id;
          return (
            <div key={ev.id}>
              <button
                onClick={() => setExpandedId(isExpanded ? null : ev.id)}
                className="w-full grid items-center px-4 py-2.5 rounded-lg text-left transition-opacity hover:opacity-90"
                style={{
                  gridTemplateColumns: "140px 1fr 80px 80px 80px",
                  background: isExpanded ? "rgba(0, 240, 255, 0.06)" : "rgba(0, 240, 255, 0.02)",
                  border: `1px solid ${isExpanded ? "rgba(0, 240, 255, 0.15)" : "rgba(0, 240, 255, 0.05)"}`,
                }}
              >
                <span className="text-[10px] font-mono text-slate-500">
                  {formatTimestamp(ev.timestamp)}
                </span>
                <span className="text-[10px] font-mono text-slate-400 truncate pr-4">
                  {ev.task_preview || "—"}
                </span>
                <span className="text-right">
                  <span
                    className="inline-flex items-center gap-1 text-[9px] font-bold uppercase"
                    style={{ color }}
                  >
                    <span
                      className="w-1.5 h-1.5 rounded-full shrink-0"
                      style={{ background: color }}
                    />
                    {ev.status}
                  </span>
                </span>
                <span className="text-[10px] font-mono text-slate-400 text-right">
                  {ev.latency_ms.toLocaleString()}ms
                </span>
                <span className="text-[10px] font-mono text-slate-400 text-right">
                  {ev.tokens_used.toLocaleString()}
                </span>
              </button>

              {/* Expanded detail */}
              {isExpanded && (
                <div
                  className="mx-4 px-4 py-3 rounded-b-lg space-y-2"
                  style={{
                    background: "rgba(0, 240, 255, 0.03)",
                    borderLeft: "1px solid rgba(0, 240, 255, 0.1)",
                    borderRight: "1px solid rgba(0, 240, 255, 0.1)",
                    borderBottom: "1px solid rgba(0, 240, 255, 0.1)",
                  }}
                >
                  <div className="flex items-center gap-4">
                    <Detail label="Provider" value={ev.provider} />
                    <Detail label="Model" value={ev.model} />
                    <Detail label="Full timestamp" value={new Date(ev.timestamp).toLocaleString()} />
                  </div>
                  {ev.task_preview && (
                    <div>
                      <span className="text-[9px] font-bold uppercase tracking-wider text-slate-600">
                        Task
                      </span>
                      <div className="text-[11px] font-mono text-slate-400 mt-1 whitespace-pre-wrap break-words">
                        {ev.task_preview}
                      </div>
                    </div>
                  )}
                  {ev.error_message && (
                    <div>
                      <span className="text-[9px] font-bold uppercase tracking-wider text-red-500">
                        Error
                      </span>
                      <div
                        className="text-[11px] font-mono mt-1 whitespace-pre-wrap break-words"
                        style={{ color: "#ff4757" }}
                      >
                        {ev.error_message}
                      </div>
                    </div>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>

      {/* Show more */}
      {visibleCount < filtered.length && (
        <button
          onClick={() => setVisibleCount((c) => c + PAGE_SIZE)}
          className="mt-4 w-full py-2 rounded-lg text-xs font-medium tracking-wide transition-opacity hover:opacity-80"
          style={{
            background: "rgba(0, 240, 255, 0.06)",
            border: "1px solid rgba(0, 240, 255, 0.12)",
            color: "#00f0ff",
          }}
        >
          Show more ({filtered.length - visibleCount} remaining)
        </button>
      )}
    </div>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div>
      <span className="text-[9px] font-bold uppercase tracking-wider text-slate-600">
        {label}
      </span>
      <div className="text-[10px] font-mono text-slate-400">{value}</div>
    </div>
  );
}

function formatTimestamp(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}
