"use client";

import { useEffect, useState, useCallback, use } from "react";
import Link from "next/link";
import { useSearchParams, useRouter } from "next/navigation";
import DatabaseSidebar from "@/components/db/DatabaseSidebar";
import {
  fetchTables,
  fetchDatabaseStats,
  type Table,
  type DatabaseStats,
} from "@/lib/api-db";
import RealtimeStatsBar from "@/components/db/RealtimeStatsBar";
import DbAiChat from "@/components/db/DbAiChat";

export default function DatabaseDetailPage({
  params,
}: {
  params: Promise<{ db: string }>;
}) {
  const { db } = use(params);
  const searchParams = useSearchParams();
  const router = useRouter();
  const selectedSchema = searchParams.get("schema") || "public";
  const activeTab = searchParams.get("tab") === "ai" ? "ai" : "tables";

  const switchTab = useCallback(
    (tab: "tables" | "ai") => {
      const params = new URLSearchParams(searchParams.toString());
      if (tab === "tables") {
        params.delete("tab");
      } else {
        params.set("tab", tab);
      }
      const qs = params.toString();
      router.push(`/db/${encodeURIComponent(db)}${qs ? `?${qs}` : ""}`);
    },
    [searchParams, router, db],
  );
  const [tables, setTables] = useState<Table[]>([]);
  const [stats, setStats] = useState<DatabaseStats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [sortCol, setSortCol] = useState<"name" | "estimated_rows" | "size">("name");
  const [sortAsc, setSortAsc] = useState(true);

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([fetchTables(db, selectedSchema), fetchDatabaseStats(db)])
      .then(([t, s]) => {
        setTables(t);
        setStats(s);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [db, selectedSchema]);

  const handleSort = (col: typeof sortCol) => {
    if (sortCol === col) {
      setSortAsc((prev) => !prev);
    } else {
      setSortCol(col);
      setSortAsc(true);
    }
  };

  const sorted = [...tables].sort((a, b) => {
    const dir = sortAsc ? 1 : -1;
    if (sortCol === "name") return a.name.localeCompare(b.name) * dir;
    if (sortCol === "estimated_rows") return (a.estimated_rows - b.estimated_rows) * dir;
    // size comparison: use string for now (API returns human-readable)
    return a.size.localeCompare(b.size) * dir;
  });

  const totalRows = tables.reduce((sum, t) => sum + t.estimated_rows, 0);

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background: "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div className="w-[1px] h-4" style={{ background: "rgba(0, 240, 255, 0.12)" }} />
          <Link
            href="/db"
            className="text-slate-400 hover:text-slate-200 text-sm font-mono transition-colors"
          >
            Database Manager
          </Link>
          <span className="text-slate-600">/</span>
          <h1 className="text-lg font-bold tracking-wider font-mono" style={{ color: "#00f0ff" }}>
            {decodeURIComponent(db)}
          </h1>
        </div>
        <a
          href={`/api/v1/${encodeURIComponent(db)}/docs`}
          target="_blank"
          rel="noopener noreferrer"
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            background: "rgba(168, 85, 247, 0.1)",
            border: "1px solid rgba(168, 85, 247, 0.3)",
            color: "#a855f7",
          }}
        >
          API Docs
        </a>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 260 }} className="shrink-0">
          <DatabaseSidebar />
        </div>

        <div className="flex-1 overflow-y-auto px-8 py-6">
          {/* ── Tab Bar ──────────────────────────────────── */}
          <div
            className="flex gap-6 mb-6"
            style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.08)" }}
          >
            {(["tables", "ai"] as const).map((tab) => {
              const isActive = activeTab === tab;
              const label = tab === "tables" ? "Tables" : "AI Query";
              return (
                <button
                  key={tab}
                  onClick={() => switchTab(tab)}
                  className="pb-2.5 text-[10px] font-bold tracking-[0.15em] uppercase font-mono transition-colors"
                  style={{
                    color: isActive ? "#00f0ff" : "#64748b",
                    borderBottom: isActive ? "2px solid #00f0ff" : "2px solid transparent",
                    marginBottom: "-1px",
                  }}
                  onMouseEnter={(e) => {
                    if (!isActive) e.currentTarget.style.color = "#94a3b8";
                  }}
                  onMouseLeave={(e) => {
                    if (!isActive) e.currentTarget.style.color = "#64748b";
                  }}
                >
                  {label}
                </button>
              );
            })}
          </div>

          {/* ── Tab: Tables ──────────────────────────────── */}
          {activeTab === "tables" && (
            <>
              {/* Error */}
              {error && (
                <div
                  className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
                  style={{
                    background: "rgba(255, 71, 87, 0.06)",
                    border: "1px solid rgba(255, 71, 87, 0.15)",
                  }}
                >
                  <span className="w-2 h-2 rounded-full shrink-0" style={{ background: "#ff4757" }} />
                  <span className="text-xs text-red-400 font-mono">{error}</span>
                </div>
              )}

              {/* Loading */}
              {loading && (
                <div className="flex items-center justify-center py-20">
                  <span className="text-slate-600 text-sm font-mono animate-pulse">Loading...</span>
                </div>
              )}

              {!loading && !error && (
                <>
                  {/* ── Realtime Metrics ──────────────────────────── */}
                  <RealtimeStatsBar db={db} />

                  {/* ── System Stats ──────────────────────────────── */}
                  {stats && (
                    <div className="mb-8">
                      <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
                        System Stats
                      </h2>

                      {/* Version banner */}
                      <div
                        className="rounded-lg px-4 py-2 mb-3 text-[10px] font-mono text-slate-400"
                        style={{
                          background: "rgba(0, 240, 255, 0.03)",
                          border: "1px solid rgba(0, 240, 255, 0.06)",
                        }}
                      >
                        {stats.version}
                      </div>

                      <div className="grid grid-cols-4 gap-3">
                        <StatCard label="Database Size" value={stats.size} accent="#00f0ff" />
                        <StatCard label="Tables" value={tables.length} accent="#a855f7" />
                        <StatCard label="Schemas" value={stats.schema_count} accent="#6366f1" />
                        <StatCard label="Total Rows" value={formatNumber(totalRows)} accent="#06d6a0" />
                        <StatCard
                          label="Connections"
                          value={`${stats.active_connections} / ${stats.max_connections}`}
                          accent="#f59e0b"
                        />
                        <StatCard
                          label="Cache Hit Ratio"
                          value={`${(stats.cache_hit_ratio * 100).toFixed(1)}%`}
                          accent={stats.cache_hit_ratio > 0.95 ? "#06d6a0" : "#ff4757"}
                        />
                        <StatCard
                          label="Commits / Rollbacks"
                          value={`${formatNumber(stats.total_commits)} / ${formatNumber(stats.total_rollbacks)}`}
                          accent="#8b5cf6"
                        />
                        <StatCard
                          label="Uptime"
                          value={formatUptime(stats.uptime_seconds)}
                          accent="#64748b"
                        />
                      </div>

                      {stats.dead_tuples > 10000 && (
                        <div
                          className="mt-3 rounded-lg px-4 py-2 text-[10px] font-mono flex items-center gap-2"
                          style={{
                            background: "rgba(255, 71, 87, 0.06)",
                            border: "1px solid rgba(255, 71, 87, 0.12)",
                            color: "#ff4757",
                          }}
                        >
                          <span className="w-1.5 h-1.5 rounded-full" style={{ background: "#ff4757" }} />
                          {formatNumber(stats.dead_tuples)} dead tuples &mdash; consider running VACUUM
                        </div>
                      )}
                    </div>
                  )}

                  {/* ── Tables ────────────────────────────────────── */}
                  <div className="flex items-center gap-2 mb-3">
                    <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500">
                      Tables
                    </h2>
                    <span
                      className="text-[9px] px-1.5 py-0.5 rounded font-mono"
                      style={{
                        color: "#6366f1",
                        background: "rgba(99, 102, 241, 0.08)",
                        border: "1px solid rgba(99, 102, 241, 0.15)",
                      }}
                    >
                      {selectedSchema}
                    </span>
                  </div>

                  {tables.length === 0 ? (
                    <div className="flex flex-col items-center justify-center py-16">
                      <p className="text-slate-600 text-sm font-mono">No tables found in this database</p>
                    </div>
                  ) : (
                    <div
                      className="rounded-lg overflow-hidden"
                      style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
                    >
                      <table className="w-full text-[11px] font-mono">
                        <thead>
                          <tr style={{ background: "rgba(0, 240, 255, 0.03)", borderBottom: "1px solid rgba(0, 240, 255, 0.08)" }}>
                            <SortHeader
                              label="Table"
                              col="name"
                              active={sortCol}
                              asc={sortAsc}
                              onClick={handleSort}
                            />
                            <th className="px-4 py-2.5 text-left text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                              Schema
                            </th>
                            <SortHeader
                              label="Rows"
                              col="estimated_rows"
                              active={sortCol}
                              asc={sortAsc}
                              onClick={handleSort}
                              align="right"
                            />
                            <SortHeader
                              label="Size"
                              col="size"
                              active={sortCol}
                              asc={sortAsc}
                              onClick={handleSort}
                              align="right"
                            />
                            <th className="px-4 py-2.5 text-center text-slate-500 font-bold tracking-wider uppercase text-[9px]">
                              PK
                            </th>
                          </tr>
                        </thead>
                        <tbody>
                          {sorted.map((t, i) => {
                            const isEven = i % 2 === 0;
                            return (
                              <tr
                                key={`${t.schema}.${t.name}`}
                                className="transition-colors hover:bg-white/[0.02]"
                                style={{
                                  background: isEven ? "transparent" : "rgba(0, 0, 0, 0.15)",
                                  borderBottom: "1px solid rgba(0, 240, 255, 0.04)",
                                }}
                              >
                                <td className="px-4 py-2">
                                  <Link
                                    href={`/db/${encodeURIComponent(db)}/${encodeURIComponent(t.name)}?schema=${encodeURIComponent(t.schema)}`}
                                    className="font-semibold transition-colors hover:underline"
                                    style={{ color: "#00f0ff" }}
                                  >
                                    {t.name}
                                  </Link>
                                </td>
                                <td className="px-4 py-2">
                                  <span
                                    className="text-[9px] px-1.5 py-0.5 rounded"
                                    style={{
                                      color: "#6366f1",
                                      background: "rgba(99, 102, 241, 0.08)",
                                      border: "1px solid rgba(99, 102, 241, 0.15)",
                                    }}
                                  >
                                    {t.schema}
                                  </span>
                                </td>
                                <td className="px-4 py-2 text-right text-slate-400">
                                  ~{formatNumber(t.estimated_rows)}
                                </td>
                                <td className="px-4 py-2 text-right text-slate-500">
                                  {t.size}
                                </td>
                                <td className="px-4 py-2 text-center">
                                  {t.has_pk ? (
                                    <span
                                      className="text-[8px] px-1.5 py-0.5 rounded font-bold"
                                      style={{
                                        color: "#06d6a0",
                                        background: "rgba(6, 214, 160, 0.08)",
                                        border: "1px solid rgba(6, 214, 160, 0.15)",
                                      }}
                                    >
                                      PK
                                    </span>
                                  ) : (
                                    <span className="text-slate-700">&mdash;</span>
                                  )}
                                </td>
                              </tr>
                            );
                          })}
                        </tbody>
                      </table>
                    </div>
                  )}
                </>
              )}
            </>
          )}

          {/* ── Tab: AI Query ────────────────────────────── */}
          {activeTab === "ai" && <DbAiChat db={db} />}
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────

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
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-xl font-bold font-mono mt-1" style={{ color: accent }}>
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}

function SortHeader({
  label,
  col,
  active,
  asc,
  onClick,
  align = "left",
}: {
  label: string;
  col: string;
  active: string;
  asc: boolean;
  onClick: (col: "name" | "estimated_rows" | "size") => void;
  align?: "left" | "right";
}) {
  const isActive = active === col;
  return (
    <th
      className={`px-4 py-2.5 text-${align} text-[9px] font-bold tracking-wider uppercase cursor-pointer select-none transition-colors hover:text-slate-300`}
      style={{ color: isActive ? "#00f0ff" : "#64748b" }}
      onClick={() => onClick(col as "name" | "estimated_rows" | "size")}
    >
      {label}
      {isActive && (
        <span className="ml-1 text-[8px]">{asc ? "\u25B2" : "\u25BC"}</span>
      )}
    </th>
  );
}

// ── Formatters ──────────────────────────────────────────────────────

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatUptime(seconds: number): string {
  const days = Math.floor(seconds / 86400);
  const hours = Math.floor((seconds % 86400) / 3600);
  if (days > 0) return `${days}d ${hours}h`;
  const mins = Math.floor((seconds % 3600) / 60);
  if (hours > 0) return `${hours}h ${mins}m`;
  return `${mins}m`;
}
