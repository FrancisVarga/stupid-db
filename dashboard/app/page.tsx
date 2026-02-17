"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import {
  fetchStats,
  fetchPageRank,
  fetchCommunities,
  fetchDegrees,
  fetchPatterns,
  fetchTrends,
  fetchAnomalies,
  fetchQueueStatus,
  type Stats,
  type QueueStatus,
  type PageRankEntry,
  type CommunityEntry,
  type DegreeEntry,
  type TemporalPattern,
  type TrendEntry,
  type AnomalyEntry,
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

function magnitudeColor(mag: number): string {
  if (mag >= 3.0) return "#ff4757";
  if (mag >= 2.0) return "#ffe600";
  return "#06d6a0";
}

const CATEGORY_COLORS: Record<string, string> = {
  Churn: "#ff4757",
  Engagement: "#06d6a0",
  ErrorChain: "#ff8a00",
  Funnel: "#00d4ff",
  Unknown: "#64748b",
};

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}

export default function DashboardPage() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [queue, setQueue] = useState<QueueStatus | null>(null);
  const [pagerank, setPagerank] = useState<PageRankEntry[]>([]);
  const [communities, setCommunities] = useState<CommunityEntry[]>([]);
  const [degrees, setDegrees] = useState<DegreeEntry[]>([]);
  const [patterns, setPatterns] = useState<TemporalPattern[]>([]);
  const [trends, setTrends] = useState<TrendEntry[]>([]);
  const [anomalies, setAnomalies] = useState<AnomalyEntry[]>([]);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    // Fire all fetches in parallel — each updates its own state independently
    fetchStats().then(setStats).catch(() => setError("Server offline"));
    fetchPageRank(10).then(setPagerank).catch(() => {});
    fetchCommunities().then(setCommunities).catch(() => {});
    fetchDegrees(10).then(setDegrees).catch(() => {});
    fetchPatterns().then(setPatterns).catch(() => {});
    fetchTrends().then(setTrends).catch(() => {});
    fetchAnomalies(20).then(setAnomalies).catch(() => {});
    fetchQueueStatus().then(setQueue).catch(() => {});
  }, []);

  const anomalousCount = anomalies.filter((a) => a.is_anomalous).length;
  const criticalAnomalies = anomalies.filter((a) => a.score >= 0.7);
  const criticalTrends = trends.filter((t) => t.magnitude >= 3.0);

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
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            stupid-db
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            dashboard
          </span>
        </div>
        <div className="flex items-center gap-3">
          <Link
            href="/queue"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.2)",
              color: "#00f0ff",
            }}
          >
            <span
              className="w-1.5 h-1.5 rounded-full"
              style={{
                background: Object.values(queue?.queues ?? {}).some((q) => q.connected) ? "#00ff88" : "#64748b",
              }}
            />
            Queue
          </Link>
          <Link
            href="/db"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(6, 214, 160, 0.08)",
              border: "1px solid rgba(6, 214, 160, 0.2)",
              color: "#06d6a0",
            }}
          >
            Database
          </Link>
          <Link
            href="/athena"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(16, 185, 129, 0.08)",
              border: "1px solid rgba(16, 185, 129, 0.2)",
              color: "#10b981",
            }}
          >
            Athena
          </Link>
          <Link
            href="/agents"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(168, 85, 247, 0.08)",
              border: "1px solid rgba(168, 85, 247, 0.2)",
              color: "#a855f7",
            }}
          >
            Agents
          </Link>
          <Link
            href="/anomaly-rules"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(249, 115, 22, 0.08)",
              border: "1px solid rgba(249, 115, 22, 0.2)",
              color: "#f97316",
            }}
          >
            Rules
          </Link>
          <Link
            href="/catalog"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(56, 189, 248, 0.08)",
              border: "1px solid rgba(56, 189, 248, 0.2)",
              color: "#38bdf8",
            }}
          >
            Catalog
          </Link>
          <Link
            href="/assistant"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(99, 102, 241, 0.08)",
              border: "1px solid rgba(99, 102, 241, 0.2)",
              color: "#6366f1",
            }}
          >
            Assistant
          </Link>
          <Link
            href="/ai-sdk"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(139, 92, 246, 0.08)",
              border: "1px solid rgba(139, 92, 246, 0.2)",
              color: "#8b5cf6",
            }}
          >
            AI SDK
          </Link>
          <Link
            href="/explore"
            className="inline-flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-90 transition-opacity"
            style={{
              background: "rgba(0, 240, 255, 0.12)",
              border: "1px solid rgba(0, 240, 255, 0.25)",
              color: "#00f0ff",
            }}
          >
            Open Explorer
          </Link>
        </div>
      </header>

      {/* Main content */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {/* Offline banner */}
        {error && (
          <div
            className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
            style={{
              background: "rgba(255, 71, 87, 0.06)",
              border: "1px solid rgba(255, 71, 87, 0.15)",
            }}
          >
            <span
              className="w-2 h-2 rounded-full shrink-0 animate-pulse"
              style={{ background: "#ff4757" }}
            />
            <span className="text-xs text-red-400 font-medium">
              {error}
            </span>
            <span className="text-[10px] text-slate-500 ml-auto">
              Start:{" "}
              <code className="text-slate-400 bg-slate-800/50 px-1.5 py-0.5 rounded">
                stupid-server serve
              </code>
            </span>
          </div>
        )}

        {/* Stats row */}
        <div className="grid grid-cols-5 gap-3 mb-6">
          <StatCard
            label="Documents"
            value={stats?.doc_count ?? "--"}
            accent="#00f0ff"
          />
          <StatCard
            label="Segments"
            value={stats?.segment_count ?? "--"}
            accent="#06d6a0"
          />
          <StatCard
            label="Nodes"
            value={stats?.node_count ?? "--"}
            accent="#a855f7"
          />
          <StatCard
            label="Edges"
            value={stats?.edge_count ?? "--"}
            accent="#f472b6"
          />
          <StatCard
            label="Anomalies"
            value={anomalousCount}
            accent={anomalousCount > 0 ? "#ff4757" : "#06d6a0"}
          />
        </div>

        {/* Entity type breakdown */}
        {stats && (
        <div className="mb-6">
          <div className="flex flex-wrap gap-1.5">
            {Object.entries(stats.nodes_by_type)
              .sort(([, a], [, b]) => b - a)
              .map(([type, count]) => {
                const color = ENTITY_COLORS[type] || "#94a3b8";
                return (
                  <span
                    key={type}
                    className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-medium tracking-wide"
                    style={{
                      background: `${color}12`,
                      color: color,
                      border: `1px solid ${color}25`,
                    }}
                  >
                    {type}
                    <span className="font-mono font-bold">
                      {count.toLocaleString()}
                    </span>
                  </span>
                );
              })}
          </div>
        </div>
        )}

        {/* Panels grid */}
        <div className="grid grid-cols-3 gap-4 mb-6">
          {/* Anomalies panel */}
          <PanelCard
            title="Anomalies"
            subtitle={`${anomalousCount} detected`}
            accentColor={anomalousCount > 0 ? "#ff4757" : "#06d6a0"}
            href="/anomalies"
          >
            {anomalies.length > 0 ? (
              <div className="space-y-1.5">
                {anomalies.slice(0, 6).map((a) => (
                  <div
                    key={a.id}
                    className="flex items-center justify-between"
                  >
                    <div className="flex items-center gap-2">
                      <span
                        className="w-1.5 h-1.5 rounded-full shrink-0"
                        style={{
                          background:
                            ENTITY_COLORS[a.entity_type] || "#888",
                        }}
                      />
                      <span className="text-[10px] font-mono text-slate-400 truncate max-w-[120px]">
                        {a.key}
                      </span>
                    </div>
                    <span
                      className="text-[10px] font-mono font-bold"
                      style={{ color: classificationColor(a.score) }}
                    >
                      {a.score.toFixed(3)}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>

          {/* Trends panel */}
          <PanelCard
            title="Trends"
            subtitle={`${criticalTrends.length} critical`}
            accentColor={criticalTrends.length > 0 ? "#ff4757" : "#06d6a0"}
            href="/explore?tab=trends"
          >
            {trends.length > 0 ? (
              <div className="space-y-1.5">
                {trends.slice(0, 6).map((t) => (
                  <div
                    key={t.metric}
                    className="flex items-center justify-between"
                  >
                    <span className="text-[10px] font-mono text-slate-400 truncate max-w-[120px]">
                      {t.metric}
                    </span>
                    <div className="flex items-center gap-2">
                      <span
                        className="text-[10px]"
                        style={{
                          color:
                            t.direction === "Up"
                              ? "#06d6a0"
                              : t.direction === "Down"
                                ? "#ff4757"
                                : "#64748b",
                        }}
                      >
                        {t.direction === "Up"
                          ? "\u2191"
                          : t.direction === "Down"
                            ? "\u2193"
                            : "\u2192"}
                      </span>
                      <span
                        className="text-[10px] font-mono font-bold"
                        style={{ color: magnitudeColor(t.magnitude) }}
                      >
                        {t.magnitude.toFixed(2)}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>

          {/* Patterns panel */}
          <PanelCard
            title="Patterns"
            subtitle={`${patterns.length} detected`}
            accentColor="#00d4ff"
            href="/patterns"
          >
            {patterns.length > 0 ? (
              <div className="space-y-1.5">
                {patterns.slice(0, 5).map((p) => {
                  const catColor =
                    CATEGORY_COLORS[p.category] || "#64748b";
                  return (
                    <div key={p.id} className="flex items-center gap-2">
                      <span
                        className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded shrink-0"
                        style={{
                          background: `${catColor}15`,
                          color: catColor,
                        }}
                      >
                        {p.category}
                      </span>
                      <span className="text-[10px] text-slate-500 font-mono truncate">
                        {p.sequence.join(" \u2192 ")}
                      </span>
                    </div>
                  );
                })}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>
        </div>

        <div className="grid grid-cols-3 gap-4 mb-6">
          {/* PageRank top influencers */}
          <PanelCard
            title="Top Influencers"
            subtitle="PageRank"
            accentColor="#a855f7"
            href="/explore?tab=pagerank"
          >
            {pagerank.length > 0 ? (
              <div className="space-y-1.5">
                {pagerank.slice(0, 6).map((p, i) => (
                  <div
                    key={p.id}
                    className="flex items-center justify-between"
                  >
                    <div className="flex items-center gap-2">
                      <span className="text-[9px] font-mono text-slate-600 w-3">
                        {i + 1}
                      </span>
                      <span
                        className="w-1.5 h-1.5 rounded-full shrink-0"
                        style={{
                          background:
                            ENTITY_COLORS[p.entity_type] || "#888",
                        }}
                      />
                      <span className="text-[10px] font-mono text-slate-400 truncate max-w-[100px]">
                        {p.key}
                      </span>
                    </div>
                    <span className="text-[10px] font-mono text-purple-400">
                      {p.score.toFixed(4)}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>

          {/* Communities */}
          <PanelCard
            title="Communities"
            subtitle={`${communities.length} clusters`}
            accentColor="#f472b6"
            href="/explore?tab=communities"
          >
            {communities.length > 0 ? (
              <div className="space-y-1.5">
                {communities.slice(0, 6).map((c) => (
                  <div
                    key={c.community_id}
                    className="flex items-center justify-between"
                  >
                    <span className="text-[10px] font-mono text-slate-400">
                      Cluster {c.community_id}
                    </span>
                    <div className="flex items-center gap-2">
                      <span className="text-[10px] font-mono text-pink-400">
                        {c.member_count} members
                      </span>
                      <span className="text-[9px] text-slate-600 truncate max-w-[80px]">
                        {c.top_nodes
                          .slice(0, 2)
                          .map((n) => n.key)
                          .join(", ")}
                      </span>
                    </div>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>

          {/* Degree centrality */}
          <PanelCard
            title="Most Connected"
            subtitle="Degree centrality"
            accentColor="#06d6a0"
            href="/explore?tab=degrees"
          >
            {degrees.length > 0 ? (
              <div className="space-y-1.5">
                {degrees.slice(0, 6).map((d, i) => (
                  <div
                    key={d.id}
                    className="flex items-center justify-between"
                  >
                    <div className="flex items-center gap-2">
                      <span className="text-[9px] font-mono text-slate-600 w-3">
                        {i + 1}
                      </span>
                      <span
                        className="w-1.5 h-1.5 rounded-full shrink-0"
                        style={{
                          background:
                            ENTITY_COLORS[d.entity_type] || "#888",
                        }}
                      />
                      <span className="text-[10px] font-mono text-slate-400 truncate max-w-[100px]">
                        {d.key}
                      </span>
                    </div>
                    <span className="text-[10px] font-mono text-emerald-400">
                      {d.total}
                    </span>
                  </div>
                ))}
              </div>
            ) : (
              <EmptyState />
            )}
          </PanelCard>
        </div>

        {/* Alerts section */}
        {(criticalAnomalies.length > 0 || criticalTrends.length > 0) && (
          <div className="mb-6">
            <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
              Active Alerts
            </h2>
            <div className="space-y-2">
              {criticalAnomalies.slice(0, 3).map((a) => (
                <AlertRow
                  key={a.id}
                  type="anomaly"
                  label={`${a.entity_type}: ${a.key}`}
                  value={`Score ${a.score.toFixed(3)}`}
                  color="#ff4757"
                />
              ))}
              {criticalTrends.slice(0, 3).map((t) => (
                <AlertRow
                  key={t.metric}
                  type="trend"
                  label={t.metric}
                  value={`${t.direction} ${t.magnitude.toFixed(2)}x`}
                  color="#ff8a00"
                />
              ))}
            </div>
          </div>
        )}

        {/* Quick actions */}
        <div className="flex items-center gap-3">
          <Link
            href="/explore"
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-90 transition-opacity"
            style={{
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
              color: "#00f0ff",
            }}
          >
            Chat Explorer
          </Link>
          <Link
            href="/explore?tab=graph"
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-90 transition-opacity"
            style={{
              background: "rgba(165, 85, 247, 0.08)",
              border: "1px solid rgba(165, 85, 247, 0.15)",
              color: "#a855f7",
            }}
          >
            Knowledge Graph
          </Link>
          <Link
            href="/reports"
            className="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-90 transition-opacity"
            style={{
              background: "rgba(6, 214, 160, 0.08)",
              border: "1px solid rgba(6, 214, 160, 0.15)",
              color: "#06d6a0",
            }}
          >
            Saved Reports
          </Link>
        </div>
      </div>
    </div>
  );
}

// ── Reusable components ──────────────────────────────────────────────

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
        {typeof value === "number" ? formatNumber(value) : value}
      </div>
    </div>
  );
}

function PanelCard({
  title,
  subtitle,
  accentColor,
  href,
  children,
}: {
  title: string;
  subtitle: string;
  accentColor: string;
  href: string;
  children: React.ReactNode;
}) {
  return (
    <Link
      href={href}
      className="block rounded-xl p-4 relative overflow-hidden hover:opacity-95 transition-opacity"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accentColor}20`,
        boxShadow: `0 0 20px ${accentColor}05`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accentColor}40, transparent)`,
        }}
      />
      <div className="flex items-center justify-between mb-3">
        <span
          className="text-xs font-bold tracking-wider uppercase"
          style={{ color: accentColor }}
        >
          {title}
        </span>
        <span className="text-[10px] text-slate-500 font-mono">
          {subtitle}
        </span>
      </div>
      {children}
    </Link>
  );
}

function AlertRow({
  type,
  label,
  value,
  color,
}: {
  type: string;
  label: string;
  value: string;
  color: string;
}) {
  return (
    <div
      className="flex items-center gap-3 px-4 py-2 rounded-lg"
      style={{
        background: `${color}08`,
        border: `1px solid ${color}20`,
      }}
    >
      <span
        className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
        style={{ background: `${color}15`, color }}
      >
        {type}
      </span>
      <span className="text-[10px] font-mono text-slate-400 flex-1 truncate">
        {label}
      </span>
      <span
        className="text-[10px] font-mono font-bold"
        style={{ color }}
      >
        {value}
      </span>
    </div>
  );
}

function EmptyState() {
  return (
    <div className="text-[10px] text-slate-600 font-mono py-2">
      No data yet — compute engine may still be processing
    </div>
  );
}
