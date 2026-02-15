"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import PatternList from "@/components/viz/PatternList";
import {
  fetchPatterns,
  fetchStats,
  type TemporalPattern,
  type Stats,
} from "@/lib/api";

const CATEGORY_COLORS: Record<string, string> = {
  Churn: "#ff4757",
  Engagement: "#06d6a0",
  ErrorChain: "#ff8a00",
  Funnel: "#00d4ff",
  Unknown: "#64748b",
};

type CategoryFilter = "all" | "Churn" | "Engagement" | "ErrorChain" | "Funnel" | "Unknown";

function formatDuration(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}

export default function PatternsPage() {
  const [patterns, setPatterns] = useState<TemporalPattern[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [categoryFilter, setCategoryFilter] = useState<CategoryFilter>("all");

  useEffect(() => {
    async function load() {
      const results = await Promise.allSettled([
        fetchPatterns(),
        fetchStats(),
      ]);
      if (results[0].status === "fulfilled") setPatterns(results[0].value);
      else setError("Server offline");
      if (results[1].status === "fulfilled") setStats(results[1].value);
      setLoading(false);
    }
    load();
  }, []);

  const filtered =
    categoryFilter === "all"
      ? patterns
      : patterns.filter((p) => p.category === categoryFilter);

  // Category counts
  const categoryCounts: Record<string, number> = {};
  for (const p of patterns) {
    categoryCounts[p.category] = (categoryCounts[p.category] || 0) + 1;
  }

  // Summary stats
  const totalMembers = patterns.reduce((acc, p) => acc + p.member_count, 0);
  const avgSupport =
    patterns.length > 0
      ? patterns.reduce((acc, p) => acc + p.support, 0) / patterns.length
      : 0;
  const avgDuration =
    patterns.length > 0
      ? patterns.reduce((acc, p) => acc + p.avg_duration_secs, 0) /
        patterns.length
      : 0;

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
            style={{ color: "#00d4ff" }}
          >
            Patterns
          </span>
        </div>
        <div className="flex items-center gap-3">
          <Link
            href="/explore?tab=patterns"
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
        className="px-6 py-3 shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <div className="flex items-center gap-6">
          <div>
            <div className="text-[9px] text-slate-600 uppercase tracking-widest">
              Patterns
            </div>
            <div className="text-lg font-bold font-mono" style={{ color: "#00d4ff" }}>
              {patterns.length}
            </div>
          </div>
          <div>
            <div className="text-[9px] text-slate-600 uppercase tracking-widest">
              Total Members
            </div>
            <div className="text-lg font-bold font-mono" style={{ color: "#a855f7" }}>
              {totalMembers.toLocaleString()}
            </div>
          </div>
          <div>
            <div className="text-[9px] text-slate-600 uppercase tracking-widest">
              Avg Support
            </div>
            <div className="text-lg font-bold font-mono" style={{ color: "#06d6a0" }}>
              {(avgSupport * 100).toFixed(1)}%
            </div>
          </div>
          <div>
            <div className="text-[9px] text-slate-600 uppercase tracking-widest">
              Avg Duration
            </div>
            <div className="text-lg font-bold font-mono" style={{ color: "#ffe600" }}>
              {formatDuration(avgDuration)}
            </div>
          </div>
          {stats && (
            <div className="ml-auto">
              <div className="text-[9px] text-slate-600 uppercase tracking-widest">
                Graph Nodes
              </div>
              <div className="text-sm font-mono text-slate-500">
                {stats.node_count.toLocaleString()}
              </div>
            </div>
          )}
        </div>
      </div>

      {/* Category filters */}
      <div
        className="px-6 py-2 flex items-center gap-2 shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.04)" }}
      >
        <span className="text-[9px] text-slate-600 uppercase tracking-widest">
          Category
        </span>
        <button
          onClick={() => setCategoryFilter("all")}
          className="text-[10px] font-bold tracking-wider uppercase px-2.5 py-1 rounded-lg transition-all"
          style={{
            color: categoryFilter === "all" ? "#00f0ff" : "#475569",
            background:
              categoryFilter === "all"
                ? "rgba(0, 240, 255, 0.08)"
                : "transparent",
            border: `1px solid ${
              categoryFilter === "all"
                ? "rgba(0, 240, 255, 0.2)"
                : "rgba(71, 85, 105, 0.15)"
            }`,
          }}
        >
          All ({patterns.length})
        </button>
        {Object.entries(CATEGORY_COLORS).map(([cat, color]) => {
          const count = categoryCounts[cat] || 0;
          if (count === 0) return null;
          const active = categoryFilter === cat;
          return (
            <button
              key={cat}
              onClick={() =>
                setCategoryFilter(
                  active ? "all" : (cat as CategoryFilter)
                )
              }
              className="text-[10px] font-bold tracking-wider uppercase px-2.5 py-1 rounded-lg transition-all"
              style={{
                color: active ? color : "#475569",
                background: active ? `${color}12` : "transparent",
                border: `1px solid ${
                  active ? `${color}30` : "rgba(71, 85, 105, 0.15)"
                }`,
              }}
            >
              {cat} ({count})
            </button>
          );
        })}
        <span className="text-[10px] text-slate-600 font-mono ml-auto">
          {filtered.length} shown
        </span>
      </div>

      {/* Main content */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-full">
            <div className="text-slate-600 text-sm animate-pulse">
              Loading patterns...
            </div>
          </div>
        ) : filtered.length > 0 ? (
          <PatternList data={filtered} />
        ) : (
          <div className="flex items-center justify-center h-full">
            <div className="text-slate-600 text-sm">
              No patterns match the current filters
            </div>
          </div>
        )}
      </div>
    </div>
  );
}
