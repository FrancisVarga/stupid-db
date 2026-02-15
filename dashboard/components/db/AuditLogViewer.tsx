"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import {
  getRuleLogs,
  type AuditLogEntry,
  type AuditLogLevel,
  type AuditExecutionPhase,
} from "@/lib/api-anomaly-rules";

const LEVEL_COLORS: Record<AuditLogLevel, string> = {
  debug: "#6b7280",
  info: "#06d6a0",
  warning: "#ffe600",
  error: "#ff4757",
};

const ALL_PHASES: AuditExecutionPhase[] = [
  "schedule_check",
  "evaluation",
  "template_match",
  "signal_check",
  "filter_apply",
  "enrichment",
  "rate_limit",
  "notification",
  "notify_error",
  "complete",
];

const DEFAULT_LIMIT = 50;
const LIMIT_INCREMENT = 50;
const AUTO_REFRESH_MS = 5000;

interface AuditLogViewerProps {
  ruleId: string;
  refreshKey: number;
}

export default function AuditLogViewer({ ruleId, refreshKey }: AuditLogViewerProps) {
  const [logs, setLogs] = useState<AuditLogEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [levelFilter, setLevelFilter] = useState<AuditLogLevel | "all">("all");
  const [phaseFilter, setPhaseFilter] = useState<AuditExecutionPhase | "all">("all");
  const [autoRefresh, setAutoRefresh] = useState(false);
  const [limit, setLimit] = useState(DEFAULT_LIMIT);

  const intervalRef = useRef<ReturnType<typeof setInterval> | null>(null);

  const fetchLogs = useCallback(() => {
    setError(null);
    getRuleLogs(ruleId, {
      level: levelFilter === "all" ? undefined : levelFilter,
      phase: phaseFilter === "all" ? undefined : phaseFilter,
      limit,
    })
      .then((data) => {
        setLogs(data);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [ruleId, levelFilter, phaseFilter, limit]);

  // Fetch on mount, refreshKey change, or filter change
  useEffect(() => {
    setLoading(true);
    fetchLogs();
  }, [refreshKey, fetchLogs]);

  // Auto-refresh interval
  useEffect(() => {
    if (autoRefresh) {
      intervalRef.current = setInterval(() => {
        fetchLogs();
      }, AUTO_REFRESH_MS);
    } else {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    }
    return () => {
      if (intervalRef.current) {
        clearInterval(intervalRef.current);
        intervalRef.current = null;
      }
    };
  }, [autoRefresh, fetchLogs]);

  const handleLoadMore = useCallback(() => {
    setLimit((prev) => prev + LIMIT_INCREMENT);
  }, []);

  const handleLevelChange = useCallback((e: React.ChangeEvent<HTMLSelectElement>) => {
    setLevelFilter(e.target.value as AuditLogLevel | "all");
  }, []);

  const handlePhaseChange = useCallback((e: React.ChangeEvent<HTMLSelectElement>) => {
    setPhaseFilter(e.target.value as AuditExecutionPhase | "all");
  }, []);

  const handleAutoRefreshToggle = useCallback(() => {
    setAutoRefresh((prev) => !prev);
  }, []);

  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: "1px solid rgba(51, 65, 85, 0.2)",
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: "linear-gradient(90deg, transparent, rgba(249, 115, 22, 0.4), transparent)" }}
      />

      {/* Header */}
      <div className="flex items-center justify-between mb-3">
        <h4 className="text-[10px] font-bold uppercase tracking-[0.15em]" style={{ color: "#f97316" }}>
          Audit Logs
        </h4>
        <button
          onClick={handleAutoRefreshToggle}
          className="px-2 py-0.5 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            color: autoRefresh ? "#06d6a0" : "#6b7280",
            border: `1px solid ${autoRefresh ? "rgba(6, 214, 160, 0.3)" : "rgba(107, 114, 128, 0.3)"}`,
            background: autoRefresh ? "rgba(6, 214, 160, 0.06)" : "rgba(107, 114, 128, 0.06)",
          }}
        >
          {autoRefresh ? "Auto-refresh ON" : "Auto-refresh OFF"}
        </button>
      </div>

      {/* Filters */}
      <div className="flex items-center gap-2 mb-3">
        <select
          value={levelFilter}
          onChange={handleLevelChange}
          className="px-2 py-1 rounded text-[10px] font-mono text-slate-400 outline-none cursor-pointer"
          style={{
            background: "rgba(255, 255, 255, 0.03)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <option value="all">All Levels</option>
          <option value="debug">Debug</option>
          <option value="info">Info</option>
          <option value="warning">Warning</option>
          <option value="error">Error</option>
        </select>

        <select
          value={phaseFilter}
          onChange={handlePhaseChange}
          className="px-2 py-1 rounded text-[10px] font-mono text-slate-400 outline-none cursor-pointer"
          style={{
            background: "rgba(255, 255, 255, 0.03)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <option value="all">All Phases</option>
          {ALL_PHASES.map((p) => (
            <option key={p} value={p}>
              {p.replace(/_/g, " ")}
            </option>
          ))}
        </select>
      </div>

      {/* Loading state */}
      {loading && (
        <div className="py-6 text-center">
          <span className="text-[10px] text-slate-600 font-mono animate-pulse">Loading logs...</span>
        </div>
      )}

      {/* Error state */}
      {error && !loading && (
        <div
          className="flex items-center gap-2 px-3 py-2 rounded-lg mb-3"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span className="w-1.5 h-1.5 rounded-full shrink-0 animate-pulse" style={{ background: "#ff4757" }} />
          <span className="text-[10px] text-red-400 font-mono">{error}</span>
        </div>
      )}

      {/* Empty state */}
      {!loading && !error && logs.length === 0 && (
        <div
          className="rounded-lg px-4 py-6 text-center"
          style={{
            background: "rgba(15, 23, 42, 0.5)",
            border: "1px solid rgba(51, 65, 85, 0.2)",
          }}
        >
          <span className="text-[10px] text-slate-600 font-mono">No audit logs found</span>
        </div>
      )}

      {/* Log entries */}
      {!loading && !error && logs.length > 0 && (
        <div className="space-y-[1px]">
          {logs.map((entry, i) => {
            const levelColor = LEVEL_COLORS[entry.level];

            return (
              <div
                key={i}
                className="flex items-start gap-2 px-3 py-1.5 font-mono"
                style={{
                  background: i % 2 === 0 ? "rgba(15, 23, 42, 0.4)" : "rgba(15, 23, 42, 0.6)",
                }}
              >
                {/* Timestamp */}
                <span className="text-[9px] text-slate-600 shrink-0 whitespace-nowrap">
                  {entry.timestamp}
                </span>

                {/* Level badge */}
                <span
                  className="text-[8px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0"
                  style={{
                    color: levelColor,
                    background: `${levelColor}15`,
                    border: `1px solid ${levelColor}30`,
                  }}
                >
                  {entry.level}
                </span>

                {/* Phase badge */}
                <span
                  className="text-[8px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded shrink-0"
                  style={{
                    color: "#a855f7",
                    background: "rgba(168, 85, 247, 0.1)",
                    border: "1px solid rgba(168, 85, 247, 0.2)",
                  }}
                >
                  {entry.phase.replace(/_/g, " ")}
                </span>

                {/* Message */}
                <span className="text-[10px] text-slate-300 flex-1 break-all">
                  {entry.message}
                </span>

                {/* Duration */}
                {entry.duration_ms != null && (
                  <span className="text-[9px] text-slate-500 shrink-0 whitespace-nowrap">
                    {entry.duration_ms}ms
                  </span>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Load more */}
      {!loading && !error && logs.length >= limit && (
        <div className="mt-3 text-center">
          <button
            onClick={handleLoadMore}
            className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#f97316",
              border: "1px solid rgba(249, 115, 22, 0.3)",
              background: "rgba(249, 115, 22, 0.06)",
            }}
          >
            Load More
          </button>
        </div>
      )}
    </div>
  );
}
