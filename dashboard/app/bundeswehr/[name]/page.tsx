"use client";

import { useEffect, useState } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import {
  fetchAgentDetail,
  fetchAgentStats,
  type AgentDetail,
  type TelemetryStat,
} from "@/lib/api";
import LogsTab from "@/components/bundeswehr/LogsTab";
import TelemetryTab from "@/components/bundeswehr/TelemetryTab";
import SessionsTab from "@/components/bundeswehr/SessionsTab";

const TIER_COLORS: Record<string, string> = {
  architect: "#ff4757",
  lead: "#a855f7",
  specialist: "#06d6a0",
};

type Tab = "overview" | "logs" | "telemetry" | "config" | "sessions";

const TABS: { key: Tab; label: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "logs", label: "Logs" },
  { key: "telemetry", label: "Telemetry" },
  { key: "config", label: "Config" },
  { key: "sessions", label: "Sessions" },
];

export default function AgentDetailPage() {
  const params = useParams<{ name: string }>();
  const agentName = decodeURIComponent(params.name);

  const [agent, setAgent] = useState<AgentDetail | null>(null);
  const [stats, setStats] = useState<TelemetryStat | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [activeTab, setActiveTab] = useState<Tab>("overview");

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      fetchAgentDetail(agentName),
      fetchAgentStats(agentName).catch(() => null),
    ])
      .then(([a, s]) => {
        setAgent(a);
        setStats(s);
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load agent"))
      .finally(() => setLoading(false));
  }, [agentName]);

  const tierColor = agent ? (TIER_COLORS[agent.tier] ?? "#64748b") : "#64748b";

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(251, 191, 36, 0.08)",
          background: "linear-gradient(180deg, rgba(251, 191, 36, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link href="/bundeswehr" className="text-slate-500 hover:text-slate-300 text-xs">
            &larr; Bundeswehr
          </Link>
          {agent && (
            <>
              <h1 className="text-lg font-bold tracking-wider" style={{ color: "#fbbf24" }}>
                {agent.name}
              </h1>
              <span
                className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
                style={{ background: `${tierColor}15`, color: tierColor }}
              >
                {agent.tier}
              </span>
              <span
                className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                style={{
                  background: "rgba(0, 240, 255, 0.06)",
                  color: "#64748b",
                  border: "1px solid rgba(0, 240, 255, 0.08)",
                }}
              >
                {agent.provider.type}/{agent.provider.model}
              </span>
            </>
          )}
        </div>
        <Link
          href="/agents"
          className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
          style={{
            background: "rgba(168, 85, 247, 0.08)",
            border: "1px solid rgba(168, 85, 247, 0.2)",
            color: "#a855f7",
          }}
        >
          Open Chat
        </Link>
      </header>

      {/* Tab bar */}
      <div
        className="px-6 flex items-center gap-1"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        {TABS.map((tab) => {
          const isActive = activeTab === tab.key;
          return (
            <button
              key={tab.key}
              onClick={() => setActiveTab(tab.key)}
              className="px-4 py-2.5 text-xs font-medium tracking-wide transition-colors relative"
              style={{
                color: isActive ? "#fbbf24" : "#64748b",
              }}
            >
              {tab.label}
              {isActive && (
                <span
                  className="absolute bottom-0 left-0 w-full h-[2px]"
                  style={{ background: "#fbbf24" }}
                />
              )}
            </button>
          );
        })}
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-5">
        {loading && (
          <div className="space-y-4">
            <div className="h-8 w-48 rounded animate-pulse" style={{ background: "rgba(0, 240, 255, 0.03)" }} />
            <div className="grid grid-cols-5 gap-3">
              {Array.from({ length: 5 }).map((_, i) => (
                <div key={i} className="h-20 rounded-xl animate-pulse" style={{ background: "rgba(0, 240, 255, 0.03)" }} />
              ))}
            </div>
          </div>
        )}

        {error && (
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
        )}

        {!loading && !error && agent && (
          <>
            {activeTab === "overview" && <OverviewTab agent={agent} stats={stats} />}
            {activeTab === "logs" && <LogsTab agentName={agentName} />}
            {activeTab === "telemetry" && <TelemetryTab agentName={agentName} />}
            {activeTab === "config" && (
              <div className="py-8 text-center text-slate-500 text-sm">
                Config editor coming soon
              </div>
            )}
            {activeTab === "sessions" && <SessionsTab agentName={agentName} />}
          </>
        )}
      </div>
    </div>
  );
}

// ── Overview Tab ───────────────────────────────────────────────

function OverviewTab({ agent, stats }: { agent: AgentDetail; stats: TelemetryStat | null }) {
  const tierColor = TIER_COLORS[agent.tier] ?? "#64748b";

  return (
    <div className="space-y-5">
      {/* Quick stats */}
      {stats && (
        <div className="grid grid-cols-5 gap-3">
          <StatCard label="Executions" value={formatNumber(stats.total_executions)} accent="#00f0ff" />
          <StatCard label="Avg Latency" value={`${Math.round(stats.avg_latency_ms)}ms`} accent="#a855f7" />
          <StatCard label="P95 Latency" value={`${Math.round(stats.p95_latency_ms)}ms`} accent="#f472b6" />
          <StatCard
            label="Error Rate"
            value={`${(stats.error_rate * 100).toFixed(1)}%`}
            accent={stats.error_rate > 0.1 ? "#ff4757" : "#06d6a0"}
          />
          <StatCard label="Total Tokens" value={formatNumber(stats.total_tokens)} accent="#fbbf24" />
        </div>
      )}

      <div className="grid grid-cols-2 gap-4">
        {/* Description */}
        <InfoPanel title="Description" accent={tierColor}>
          <div className="text-xs text-slate-400 leading-relaxed whitespace-pre-wrap">
            {agent.description}
          </div>
        </InfoPanel>

        {/* Provider / Model */}
        <InfoPanel title="Provider" accent="#00f0ff">
          <div className="space-y-2">
            <Detail label="Type" value={agent.provider.type} />
            <Detail label="Model" value={agent.provider.model} />
          </div>
        </InfoPanel>
      </div>

      {/* Tags */}
      {agent.tags.length > 0 && (
        <InfoPanel title="Tags" accent="#a855f7">
          <div className="flex flex-wrap gap-1.5">
            {agent.tags.map((tag) => (
              <span
                key={tag}
                className="text-[10px] font-mono px-2 py-0.5 rounded-full"
                style={{
                  background: "rgba(168, 85, 247, 0.08)",
                  color: "#a855f7",
                  border: "1px solid rgba(168, 85, 247, 0.15)",
                }}
              >
                {tag}
              </span>
            ))}
          </div>
        </InfoPanel>
      )}

      {/* Group */}
      {agent.group && (
        <InfoPanel title="Group" accent="#fbbf24">
          <span
            className="text-[10px] font-mono px-2 py-0.5 rounded-full"
            style={{
              background: "rgba(251, 191, 36, 0.08)",
              color: "#fbbf24",
              border: "1px solid rgba(251, 191, 36, 0.15)",
            }}
          >
            {agent.group}
          </span>
        </InfoPanel>
      )}

      {/* Skills */}
      {agent.skills.length > 0 && (
        <InfoPanel title="Skills" accent="#06d6a0">
          <div className="space-y-2">
            {agent.skills.map((skill) => (
              <div key={skill.name}>
                <span className="text-[10px] font-bold text-slate-300">{skill.name}</span>
                <div className="text-[10px] font-mono text-slate-600 mt-0.5 truncate">
                  {skill.prompt.length > 120 ? skill.prompt.slice(0, 120) + "\u2026" : skill.prompt}
                </div>
              </div>
            ))}
          </div>
        </InfoPanel>
      )}
    </div>
  );
}

// ── Reusable components ────────────────────────────────────────

function StatCard({ label, value, accent }: { label: string; value: string; accent: string }) {
  return (
    <div className="rounded-xl p-4 relative overflow-hidden" style={{
      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
      border: `1px solid ${accent}20`,
    }}>
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-2xl font-bold font-mono mt-1" style={{ color: accent }}>{value}</div>
    </div>
  );
}

function InfoPanel({ title, accent, children }: { title: string; accent: string; children: React.ReactNode }) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accent}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div
        className="text-[10px] font-bold tracking-[0.15em] uppercase mb-3"
        style={{ color: accent }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function Detail({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex items-center gap-3">
      <span className="text-[9px] font-bold uppercase tracking-wider text-slate-600 w-12">{label}</span>
      <span className="text-xs font-mono text-slate-400">{value}</span>
    </div>
  );
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return n.toLocaleString();
}
