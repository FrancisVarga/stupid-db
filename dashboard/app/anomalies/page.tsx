"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import AnomalyChart from "@/components/viz/AnomalyChart";
import {
  fetchAnomalies,
  fetchStats,
  type AnomalyEntry,
  type Stats,
} from "@/lib/api";

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

function classificationColor(score: number): string {
  if (score >= 0.7) return "#ff4757";
  if (score >= 0.5) return "#ff8a00";
  if (score >= 0.3) return "#ffe600";
  return "#06d6a0";
}

function classificationLabel(score: number): string {
  if (score >= 0.7) return "CRITICAL";
  if (score >= 0.5) return "ANOMALOUS";
  if (score >= 0.3) return "MILD";
  return "NORMAL";
}

type FilterLevel = "all" | "critical" | "anomalous" | "mild" | "normal";

export default function AnomaliesPage() {
  const [anomalies, setAnomalies] = useState<AnomalyEntry[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [limit, setLimit] = useState(100);
  const [filter, setFilter] = useState<FilterLevel>("all");
  const [entityFilter, setEntityFilter] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    fetchAnomalies(limit)
      .then((a) => { setAnomalies(a); setLoading(false); })
      .catch(() => { setError("Server offline"); setLoading(false); });
    fetchStats().then(setStats).catch(() => {});
  }, [limit]);

  const filtered = anomalies.filter((a) => {
    if (entityFilter && a.entity_type !== entityFilter) return false;
    if (filter === "critical") return a.score >= 0.7;
    if (filter === "anomalous") return a.score >= 0.5 && a.score < 0.7;
    if (filter === "mild") return a.score >= 0.3 && a.score < 0.5;
    if (filter === "normal") return a.score < 0.3;
    return true;
  });

  const criticalCount = anomalies.filter((a) => a.score >= 0.7).length;
  const anomalousCount = anomalies.filter(
    (a) => a.score >= 0.5 && a.score < 0.7
  ).length;
  const mildCount = anomalies.filter(
    (a) => a.score >= 0.3 && a.score < 0.5
  ).length;
  const normalCount = anomalies.filter((a) => a.score < 0.3).length;

  // Unique entity types present in data
  const entityTypes = [...new Set(anomalies.map((a) => a.entity_type))].sort();

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link href="/" className="hover:opacity-80 transition-opacity">
            <span
              className="text-lg font-bold tracking-wider"
              style={{ color: "#00f0ff" }}
            >
              stupid-db
            </span>
          </Link>
          <span className="text-slate-600">/</span>
          <span
            className="text-sm font-bold tracking-wider uppercase"
            style={{ color: "#ff4757" }}
          >
            Anomalies
          </span>
        </div>
        <div className="flex items-center gap-3">
          <Link
            href="/explore?tab=anomalies"
            className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg hover:opacity-90 transition-opacity"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.12)",
            }}
          >
            Open in Explorer
          </Link>
        </div>
      </header>

      {/* Offline banner */}
      {error && (
        <div
          className="mx-6 mt-3 flex items-center gap-3 px-4 py-2.5 rounded-lg"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span
            className="w-2 h-2 rounded-full shrink-0 animate-pulse"
            style={{ background: "#ff4757" }}
          />
          <span className="text-xs text-red-400 font-medium">{error}</span>
        </div>
      )}

      {/* Stats bar */}
      <div
        className="px-6 py-3 shrink-0 flex items-center gap-4"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <StatPill
          label="Critical"
          count={criticalCount}
          color="#ff4757"
          active={filter === "critical"}
          onClick={() =>
            setFilter((f) => (f === "critical" ? "all" : "critical"))
          }
        />
        <StatPill
          label="Anomalous"
          count={anomalousCount}
          color="#ff8a00"
          active={filter === "anomalous"}
          onClick={() =>
            setFilter((f) => (f === "anomalous" ? "all" : "anomalous"))
          }
        />
        <StatPill
          label="Mild"
          count={mildCount}
          color="#ffe600"
          active={filter === "mild"}
          onClick={() => setFilter((f) => (f === "mild" ? "all" : "mild"))}
        />
        <StatPill
          label="Normal"
          count={normalCount}
          color="#06d6a0"
          active={filter === "normal"}
          onClick={() =>
            setFilter((f) => (f === "normal" ? "all" : "normal"))
          }
        />
        <div className="h-4 w-px bg-slate-800 mx-1" />
        <span className="text-[10px] text-slate-500 font-mono">
          {filtered.length} / {anomalies.length} shown
        </span>
        {stats && (
          <span className="text-[10px] text-slate-600 font-mono ml-auto">
            {stats.node_count.toLocaleString()} nodes in graph
          </span>
        )}
      </div>

      {/* Entity type filter */}
      {entityTypes.length > 1 && (
        <div
          className="px-6 py-2 flex items-center gap-2 shrink-0"
          style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.04)" }}
        >
          <span className="text-[9px] text-slate-600 uppercase tracking-widest">
            Entity
          </span>
          <button
            onClick={() => setEntityFilter(null)}
            className="text-[10px] font-bold tracking-wider uppercase px-2 py-0.5 rounded-lg transition-all"
            style={{
              color: !entityFilter ? "#00f0ff" : "#475569",
              background: !entityFilter
                ? "rgba(0, 240, 255, 0.08)"
                : "transparent",
              border: `1px solid ${
                !entityFilter
                  ? "rgba(0, 240, 255, 0.2)"
                  : "rgba(71, 85, 105, 0.15)"
              }`,
            }}
          >
            All
          </button>
          {entityTypes.map((et) => {
            const color = ENTITY_COLORS[et] || "#94a3b8";
            const active = entityFilter === et;
            return (
              <button
                key={et}
                onClick={() => setEntityFilter(active ? null : et)}
                className="text-[10px] font-bold tracking-wider uppercase px-2 py-0.5 rounded-lg transition-all"
                style={{
                  color: active ? color : "#475569",
                  background: active ? `${color}12` : "transparent",
                  border: `1px solid ${
                    active ? `${color}30` : "rgba(71, 85, 105, 0.15)"
                  }`,
                }}
              >
                {et}
              </button>
            );
          })}
        </div>
      )}

      {/* Main content */}
      <div className="flex-1 flex min-h-0">
        {/* Chart area */}
        <div className="flex-1 overflow-y-auto">
          {loading ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-slate-600 text-sm animate-pulse">
                Loading anomalies...
              </div>
            </div>
          ) : filtered.length > 0 ? (
            <AnomalyChart data={filtered} />
          ) : (
            <div className="flex items-center justify-center h-full">
              <div className="text-slate-600 text-sm">
                No anomalies match the current filters
              </div>
            </div>
          )}
        </div>

        {/* Side panel — top anomalies list */}
        <div
          className="w-72 shrink-0 overflow-y-auto p-4"
          style={{ borderLeft: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          <div className="flex items-center justify-between mb-3">
            <span className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500">
              Top Anomalies
            </span>
            <select
              value={limit}
              onChange={(e) => setLimit(Number(e.target.value))}
              className="text-[10px] font-mono bg-transparent text-slate-400 border border-slate-800 rounded px-1 py-0.5"
            >
              <option value={50}>50</option>
              <option value={100}>100</option>
              <option value={200}>200</option>
              <option value={500}>500</option>
            </select>
          </div>
          <div className="space-y-1">
            {filtered.slice(0, 30).map((a) => (
              <div
                key={a.id}
                className="flex items-center justify-between py-1.5 px-2 rounded-lg hover:bg-white/[0.02] transition-colors"
              >
                <div className="flex items-center gap-2 min-w-0">
                  <span
                    className="w-1.5 h-1.5 rounded-full shrink-0"
                    style={{
                      background: ENTITY_COLORS[a.entity_type] || "#888",
                    }}
                  />
                  <span className="text-[10px] font-mono text-slate-400 truncate">
                    {a.key}
                  </span>
                </div>
                <div className="flex items-center gap-1.5 shrink-0 ml-2">
                  <span
                    className="text-[9px] font-mono font-bold px-1.5 py-0.5 rounded"
                    style={{
                      background: classificationColor(a.score) + "15",
                      color: classificationColor(a.score),
                    }}
                  >
                    {classificationLabel(a.score)}
                  </span>
                  <span
                    className="text-[10px] font-mono font-bold"
                    style={{ color: classificationColor(a.score) }}
                  >
                    {a.score.toFixed(3)}
                  </span>
                </div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

function StatPill({
  label,
  count,
  color,
  active,
  onClick,
}: {
  label: string;
  count: number;
  color: string;
  active: boolean;
  onClick: () => void;
}) {
  return (
    <button
      onClick={onClick}
      className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-[10px] font-bold tracking-wider uppercase transition-all"
      style={{
        background: active ? `${color}15` : `${color}08`,
        color: active ? color : `${color}80`,
        border: `1px solid ${active ? `${color}40` : `${color}15`}`,
      }}
    >
      <span
        className="w-1.5 h-1.5 rounded-full"
        style={{ background: color }}
      />
      {label}
      <span className="font-mono">{count}</span>
    </button>
  );
}
