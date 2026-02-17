"use client";

import { useEffect, useState, useCallback, useRef } from "react";
import {
  getAthenaQueryLog,
  type QueryLogResponse,
  type AthenaQueryLogEntry,
  type DailyCostSummary,
} from "@/lib/db/athena-connections";
import CodeBlock from "./CodeBlock";

interface AthenaQueryLogProps {
  connectionId: string;
  refreshKey?: number;
}

type Tab = "costs" | "log";
type SourceFilter = "" | "user_query" | "schema_refresh_databases" | "schema_refresh_tables" | "schema_refresh_describe";
type OutcomeFilter = "" | "succeeded" | "failed" | "cancelled" | "timed_out";

export default function AthenaQueryLog({
  connectionId,
  refreshKey = 0,
}: AthenaQueryLogProps) {
  const [tab, setTab] = useState<Tab>("costs");
  const [data, setData] = useState<QueryLogResponse | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Log filters
  const [sourceFilter, setSourceFilter] = useState<SourceFilter>("");
  const [outcomeFilter, setOutcomeFilter] = useState<OutcomeFilter>("");
  const [sqlSearch, setSqlSearch] = useState("");

  const fetchLog = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const params: Record<string, string | number> = { limit: 200 };
      if (sourceFilter) params.source = sourceFilter;
      if (outcomeFilter) params.outcome = outcomeFilter;
      if (sqlSearch.trim()) params.sql_contains = sqlSearch.trim();
      const res = await getAthenaQueryLog(connectionId, params);
      setData(res);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setLoading(false);
    }
  }, [connectionId, sourceFilter, outcomeFilter, sqlSearch]);

  useEffect(() => {
    fetchLog();
  }, [fetchLog, refreshKey]);

  if (loading && !data) {
    return (
      <div className="flex items-center justify-center py-12">
        <span className="text-slate-600 text-xs font-mono animate-pulse">
          Loading query log...
        </span>
      </div>
    );
  }

  if (error && !data) {
    return (
      <div
        className="mx-4 mt-3 flex items-center gap-3 px-4 py-2.5 rounded-lg"
        style={{
          background: "rgba(255, 71, 87, 0.06)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
        }}
      >
        <span className="text-xs text-red-400 font-medium">{error}</span>
      </div>
    );
  }

  const summary = data?.summary;
  const entries = data?.entries ?? [];

  return (
    <div className="flex flex-col h-full">
      {/* Tab bar */}
      <div className="flex items-center gap-1 px-4 pt-3 pb-2 shrink-0">
        <TabButton active={tab === "costs"} onClick={() => setTab("costs")}>
          Costs
        </TabButton>
        <TabButton active={tab === "log"} onClick={() => setTab("log")}>
          Query Log
          {summary && summary.total_queries > 0 && (
            <span className="ml-1.5 text-[9px] text-slate-600">
              ({summary.total_queries})
            </span>
          )}
        </TabButton>
        <div className="flex-1" />
        <button
          onClick={fetchLog}
          disabled={loading}
          className="text-[9px] font-mono text-slate-600 hover:text-slate-400 transition-colors disabled:opacity-40"
        >
          {loading ? "..." : "Refresh"}
        </button>
      </div>

      {/* Content */}
      {tab === "costs" ? (
        <CostPanel summary={summary} />
      ) : (
        <LogPanel
          entries={entries}
          sourceFilter={sourceFilter}
          outcomeFilter={outcomeFilter}
          sqlSearch={sqlSearch}
          onSourceChange={setSourceFilter}
          onOutcomeChange={setOutcomeFilter}
          onSqlSearchChange={setSqlSearch}
        />
      )}
    </div>
  );
}

// ── Cost Panel ─────────────────────────────────────────────────────

function CostPanel({
  summary,
}: {
  summary: QueryLogResponse["summary"] | undefined;
}) {
  if (!summary || summary.total_queries === 0) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <span className="text-slate-600 text-sm font-mono">
          No queries recorded yet
        </span>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-y-auto px-4 pb-4">
      {/* Summary cards */}
      <div className="grid grid-cols-3 gap-3 mb-5">
        <CostCard
          label="Total Cost"
          value={formatCost(summary.total_cost_usd)}
          accent="#f59e0b"
        />
        <CostCard
          label="Data Scanned"
          value={formatBytes(summary.total_bytes_scanned)}
          accent="#3b82f6"
        />
        <CostCard
          label="Total Queries"
          value={summary.total_queries.toLocaleString()}
          accent="#10b981"
        />
      </div>

      {/* Daily breakdown */}
      <h3 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
        Daily Breakdown
      </h3>

      {summary.daily.length === 0 ? (
        <span className="text-xs text-slate-600 font-mono">No daily data</span>
      ) : (
        <>
          <DailyCostChart daily={summary.daily} />
          <div className="mt-3 space-y-1">
            {summary.daily.map((day) => (
              <DailyRow key={day.date} day={day} />
            ))}
          </div>
        </>
      )}
    </div>
  );
}

function CostCard({
  label,
  value,
  accent,
}: {
  label: string;
  value: string;
  accent: string;
}) {
  return (
    <div className="rounded-xl p-3 relative overflow-hidden" style={{
      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
      border: `1px solid ${accent}20`,
    }}>
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-500 text-[9px] uppercase tracking-widest">{label}</div>
      <div className="text-lg font-bold font-mono mt-0.5" style={{ color: accent }}>
        {value}
      </div>
    </div>
  );
}

function DailyCostChart({ daily }: { daily: DailyCostSummary[] }) {
  const chartRef = useRef<HTMLDivElement>(null);
  const maxCost = Math.max(...daily.map((d) => d.total_cost_usd), 0.001);

  return (
    <div ref={chartRef} className="flex items-end gap-1 h-20">
      {daily.slice().reverse().map((day) => {
        const height = Math.max((day.total_cost_usd / maxCost) * 100, 2);
        return (
          <div
            key={day.date}
            className="flex-1 relative group"
            style={{ height: "100%" }}
          >
            <div
              className="absolute bottom-0 w-full rounded-t transition-all"
              style={{
                height: `${height}%`,
                background: "linear-gradient(180deg, #f59e0b 0%, #d97706 100%)",
                opacity: 0.7,
              }}
            />
            {/* Tooltip */}
            <div className="absolute bottom-full left-1/2 -translate-x-1/2 mb-1 hidden group-hover:block z-10">
              <div
                className="rounded px-2 py-1 text-[9px] font-mono whitespace-nowrap"
                style={{
                  background: "#1e293b",
                  border: "1px solid #334155",
                  color: "#e2e8f0",
                }}
              >
                <div className="font-bold">{day.date}</div>
                <div style={{ color: "#f59e0b" }}>{formatCost(day.total_cost_usd)}</div>
                <div className="text-slate-400">{day.query_count} queries</div>
                <div className="text-slate-400">{formatBytes(day.total_bytes_scanned)}</div>
              </div>
            </div>
            {/* Date label */}
            <div className="absolute -bottom-4 left-1/2 -translate-x-1/2 text-[7px] text-slate-700 font-mono whitespace-nowrap">
              {day.date.slice(5)}
            </div>
          </div>
        );
      })}
    </div>
  );
}

function DailyRow({ day }: { day: DailyCostSummary }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center justify-between px-3 py-2 rounded-lg transition-colors hover:bg-white/[0.02]"
        style={{ border: "1px solid rgba(30, 41, 59, 0.3)" }}
      >
        <div className="flex items-center gap-3">
          <span className="text-[10px] font-mono text-slate-400 w-20">{day.date}</span>
          <span className="text-[10px] font-mono text-slate-500">
            {day.query_count} queries
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-[9px] font-mono text-slate-600">
            {formatBytes(day.total_bytes_scanned)}
          </span>
          <span className="text-[10px] font-mono font-bold" style={{ color: "#f59e0b" }}>
            {formatCost(day.total_cost_usd)}
          </span>
        </div>
      </button>
      {expanded && Object.keys(day.by_source).length > 0 && (
        <div className="ml-6 mt-1 mb-2 space-y-0.5">
          {Object.entries(day.by_source).map(([source, cost]) => (
            <div key={source} className="flex items-center justify-between px-2 py-0.5">
              <span className="text-[9px] font-mono text-slate-600">
                {formatSource(source)}
              </span>
              <span className="text-[9px] font-mono" style={{ color: "#f59e0b" }}>
                {formatCost(cost)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

// ── Log Panel ──────────────────────────────────────────────────────

function LogPanel({
  entries,
  sourceFilter,
  outcomeFilter,
  sqlSearch,
  onSourceChange,
  onOutcomeChange,
  onSqlSearchChange,
}: {
  entries: AthenaQueryLogEntry[];
  sourceFilter: SourceFilter;
  outcomeFilter: OutcomeFilter;
  sqlSearch: string;
  onSourceChange: (v: SourceFilter) => void;
  onOutcomeChange: (v: OutcomeFilter) => void;
  onSqlSearchChange: (v: string) => void;
}) {
  const [expandedId, setExpandedId] = useState<number | null>(null);

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* Filters */}
      <div className="flex items-center gap-2 px-4 pb-2 shrink-0 flex-wrap">
        <FilterSelect
          value={sourceFilter}
          onChange={(v) => onSourceChange(v as SourceFilter)}
          options={[
            { value: "", label: "All Sources" },
            { value: "user_query", label: "User Query" },
            { value: "schema_refresh_databases", label: "Schema: DBs" },
            { value: "schema_refresh_tables", label: "Schema: Tables" },
            { value: "schema_refresh_describe", label: "Schema: Describe" },
          ]}
        />
        <FilterSelect
          value={outcomeFilter}
          onChange={(v) => onOutcomeChange(v as OutcomeFilter)}
          options={[
            { value: "", label: "All Outcomes" },
            { value: "succeeded", label: "Succeeded" },
            { value: "failed", label: "Failed" },
            { value: "cancelled", label: "Cancelled" },
            { value: "timed_out", label: "Timed Out" },
          ]}
        />
        <input
          type="text"
          value={sqlSearch}
          onChange={(e) => onSqlSearchChange(e.target.value)}
          placeholder="Search SQL..."
          spellCheck={false}
          className="bg-transparent text-[10px] text-slate-300 font-mono rounded px-2 py-1 outline-none flex-1 min-w-[120px]"
          style={{
            background: "rgba(6, 8, 13, 0.4)",
            border: "1px solid rgba(30, 41, 59, 0.5)",
          }}
        />
      </div>

      {/* Entries */}
      <div className="flex-1 overflow-y-auto px-4 pb-4">
        {entries.length === 0 ? (
          <div className="flex items-center justify-center py-12">
            <span className="text-slate-600 text-sm font-mono">
              No log entries found
            </span>
          </div>
        ) : (
          <div className="space-y-1">
            {entries.map((entry) => (
              <LogEntry
                key={entry.entry_id}
                entry={entry}
                expanded={expandedId === entry.entry_id}
                onToggle={() =>
                  setExpandedId(
                    expandedId === entry.entry_id ? null : entry.entry_id,
                  )
                }
              />
            ))}
          </div>
        )}
      </div>
    </div>
  );
}

function LogEntry({
  entry,
  expanded,
  onToggle,
}: {
  entry: AthenaQueryLogEntry;
  expanded: boolean;
  onToggle: () => void;
}) {
  const outcomeColor = getOutcomeColor(entry.outcome);
  const isUserQuery = entry.source === "user_query";

  return (
    <div>
      <button
        onClick={onToggle}
        className="w-full flex items-center gap-2 px-3 py-2 rounded-lg text-left transition-colors hover:bg-white/[0.02]"
        style={{ border: `1px solid ${outcomeColor}15` }}
      >
        {/* Outcome dot */}
        <span
          className="w-1.5 h-1.5 rounded-full shrink-0"
          style={{ background: outcomeColor }}
        />

        {/* Source badge */}
        <span
          className="text-[8px] font-mono font-bold uppercase tracking-wider px-1 py-0.5 rounded shrink-0"
          style={{
            background: isUserQuery
              ? "rgba(59, 130, 246, 0.1)"
              : "rgba(100, 116, 139, 0.1)",
            color: isUserQuery ? "#3b82f6" : "#64748b",
          }}
        >
          {isUserQuery ? "USER" : "SCHEMA"}
        </span>

        {/* SQL preview */}
        <span className="text-[10px] font-mono text-slate-400 truncate flex-1 min-w-0">
          {entry.sql.length > 80 ? entry.sql.slice(0, 80) + "..." : entry.sql}
        </span>

        {/* Cost */}
        {entry.estimated_cost_usd > 0 && (
          <span className="text-[9px] font-mono font-bold shrink-0" style={{ color: "#f59e0b" }}>
            {formatCost(entry.estimated_cost_usd)}
          </span>
        )}

        {/* Duration */}
        <span className="text-[9px] font-mono text-slate-600 shrink-0 w-14 text-right">
          {formatDuration(entry.wall_clock_ms)}
        </span>

        {/* Time */}
        <span className="text-[9px] font-mono text-slate-700 shrink-0">
          {formatTime(entry.started_at)}
        </span>
      </button>

      {/* Expanded details */}
      {expanded && (
        <div
          className="mx-3 mt-1 mb-2 p-3 rounded-lg"
          style={{
            background: "rgba(6, 8, 13, 0.4)",
            border: "1px solid rgba(30, 41, 59, 0.3)",
          }}
        >
          {/* SQL */}
          <div className="mb-3">
            <div className="text-[9px] text-slate-600 font-mono uppercase tracking-wider mb-1">
              SQL
            </div>
            <CodeBlock code={entry.sql} language="sql" maxHeight="160px" />
          </div>

          {/* Stats grid */}
          <div className="grid grid-cols-2 gap-x-6 gap-y-1">
            <DetailRow label="Outcome" value={entry.outcome} color={outcomeColor} />
            <DetailRow label="Source" value={formatSource(entry.source)} />
            <DetailRow label="Database" value={entry.database} />
            <DetailRow label="Workgroup" value={entry.workgroup} />
            <DetailRow label="Data Scanned" value={formatBytes(entry.data_scanned_bytes)} />
            <DetailRow label="Cost" value={formatCost(entry.estimated_cost_usd)} color="#f59e0b" />
            <DetailRow label="Engine Time" value={`${entry.engine_execution_time_ms}ms`} />
            <DetailRow label="Wall Clock" value={formatDuration(entry.wall_clock_ms)} />
            {entry.total_rows != null && (
              <DetailRow label="Rows" value={entry.total_rows.toLocaleString()} />
            )}
            {entry.query_execution_id && (
              <DetailRow label="Query ID" value={entry.query_execution_id} />
            )}
          </div>

          {/* Error */}
          {entry.error_message && (
            <div className="mt-2 px-2 py-1.5 rounded text-[10px] font-mono text-red-400" style={{
              background: "rgba(255, 71, 87, 0.06)",
              border: "1px solid rgba(255, 71, 87, 0.15)",
            }}>
              {entry.error_message}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function DetailRow({
  label,
  value,
  color,
}: {
  label: string;
  value: string;
  color?: string;
}) {
  return (
    <div className="flex items-center justify-between">
      <span className="text-[9px] text-slate-600 font-mono">{label}</span>
      <span
        className="text-[9px] font-mono font-bold"
        style={{ color: color || "#94a3b8" }}
      >
        {value}
      </span>
    </div>
  );
}

function FilterSelect({
  value,
  onChange,
  options,
}: {
  value: string;
  onChange: (v: string) => void;
  options: { value: string; label: string }[];
}) {
  return (
    <select
      value={value}
      onChange={(e) => onChange(e.target.value)}
      className="text-[10px] font-mono text-slate-300 rounded px-2 py-1 outline-none cursor-pointer"
      style={{
        background: "rgba(6, 8, 13, 0.4)",
        border: "1px solid rgba(30, 41, 59, 0.5)",
      }}
    >
      {options.map((opt) => (
        <option key={opt.value} value={opt.value} style={{ background: "#0c1018" }}>
          {opt.label}
        </option>
      ))}
    </select>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all"
      style={{
        background: active ? "rgba(16, 185, 129, 0.1)" : "transparent",
        border: active
          ? "1px solid rgba(16, 185, 129, 0.3)"
          : "1px solid transparent",
        color: active ? "#10b981" : "#64748b",
      }}
    >
      {children}
    </button>
  );
}

// ── Formatting helpers ─────────────────────────────────────────────

function formatCost(usd: number): string {
  if (usd === 0) return "$0.00";
  if (usd < 0.01) return `$${usd.toFixed(6)}`;
  if (usd < 1) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  const i = Math.floor(Math.log(bytes) / Math.log(1024));
  const val = bytes / Math.pow(1024, i);
  return `${val.toFixed(i > 1 ? 1 : 0)} ${units[i]}`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}m ${rem.toFixed(0)}s`;
}

function formatTime(iso: string): string {
  try {
    const d = new Date(iso);
    return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return iso;
  }
}

function formatSource(source: string): string {
  const map: Record<string, string> = {
    user_query: "User Query",
    schema_refresh_databases: "Schema: Databases",
    schema_refresh_tables: "Schema: Tables",
    schema_refresh_describe: "Schema: Describe",
  };
  return map[source] || source;
}

function getOutcomeColor(outcome: string): string {
  switch (outcome) {
    case "succeeded": return "#06d6a0";
    case "failed": return "#ef4444";
    case "cancelled": return "#94a3b8";
    case "timed_out": return "#f59e0b";
    default: return "#64748b";
  }
}
