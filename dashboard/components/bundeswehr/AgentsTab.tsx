"use client";

import Link from "next/link";
import type { AgentInfo, TelemetryStat } from "@/lib/api";

const TIER_CONFIG: Record<string, { color: string; label: string; order: number }> = {
  architect: { color: "#ff4757", label: "T1 Architect", order: 1 },
  lead: { color: "#a855f7", label: "T2 Lead", order: 2 },
  specialist: { color: "#06d6a0", label: "T3 Specialist", order: 3 },
};

function tierOf(tier: string) {
  return TIER_CONFIG[tier.toLowerCase()] ?? { color: "#64748b", label: tier, order: 9 };
}

export default function AgentsTab({
  tiers,
  telemetryMap,
  groupMap,
  loading,
  error,
  filtered,
  search,
}: {
  tiers: [string, AgentInfo[]][];
  telemetryMap: Map<string, TelemetryStat>;
  groupMap: Map<string, string[]>;
  loading: boolean;
  error: string | null;
  filtered: AgentInfo[];
  search: string;
}) {
  if (loading) {
    return (
      <div className="grid grid-cols-3 gap-4">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="h-36 rounded-xl animate-pulse"
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

  if (filtered.length === 0) {
    return (
      <div className="py-12 text-center">
        <div className="text-slate-500 text-sm mb-1">No agents found</div>
        <div className="text-slate-600 text-xs font-mono">
          {search ? "Try a different search term" : "No agents configured yet"}
        </div>
      </div>
    );
  }

  return (
    <>
      {tiers.map(([tier, tierAgents]) => {
        const cfg = tierOf(tier);
        return (
          <div key={tier} className="mb-6">
            <div className="flex items-center gap-2 mb-3">
              <span
                className="text-[10px] font-bold uppercase tracking-[0.15em]"
                style={{ color: cfg.color }}
              >
                {cfg.label}
              </span>
              <span className="text-[10px] font-mono text-slate-600">
                {tierAgents.length}
              </span>
            </div>
            <div className="grid grid-cols-3 gap-4">
              {tierAgents.map((agent) => (
                <AgentCard
                  key={agent.name}
                  agent={agent}
                  stats={telemetryMap.get(agent.name)}
                  groups={groupMap.get(agent.name)}
                />
              ))}
            </div>
          </div>
        );
      })}
    </>
  );
}

function AgentCard({
  agent,
  stats,
  groups,
}: {
  agent: AgentInfo;
  stats?: TelemetryStat;
  groups?: string[];
}) {
  const cfg = tierOf(agent.tier);

  return (
    <Link
      href={`/bundeswehr/${encodeURIComponent(agent.name)}`}
      className="block rounded-xl p-4 relative overflow-hidden hover:opacity-95 transition-opacity"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${cfg.color}20`,
        boxShadow: `0 0 20px ${cfg.color}05`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${cfg.color}40, transparent)`,
        }}
      />

      {/* Name + tier */}
      <div className="flex items-center justify-between mb-2">
        <span className="text-xs font-bold text-slate-200 truncate">{agent.name}</span>
        <span
          className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded shrink-0 ml-2"
          style={{ background: `${cfg.color}15`, color: cfg.color }}
        >
          {agent.tier}
        </span>
      </div>

      {/* Description */}
      <div className="text-[10px] text-slate-500 font-mono mb-3 line-clamp-2">
        {agent.description.length > 80
          ? agent.description.slice(0, 80) + "\u2026"
          : agent.description}
      </div>

      {/* Mini stats */}
      {stats && (
        <div className="flex items-center gap-3 mb-2">
          <MiniStat label="Exec" value={String(stats.total_executions)} color="#00f0ff" />
          <MiniStat label="Avg" value={`${Math.round(stats.avg_latency_ms)}ms`} color="#a855f7" />
          <MiniStat
            label="Err"
            value={`${(stats.error_rate * 100).toFixed(1)}%`}
            color={stats.error_rate > 0.1 ? "#ff4757" : "#06d6a0"}
          />
        </div>
      )}

      {/* Group tags */}
      {groups && groups.length > 0 && (
        <div className="flex flex-wrap gap-1">
          {groups.map((g) => (
            <span
              key={g}
              className="text-[9px] font-mono px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(251, 191, 36, 0.08)",
                color: "#fbbf24",
                border: "1px solid rgba(251, 191, 36, 0.15)",
              }}
            >
              {g}
            </span>
          ))}
        </div>
      )}
    </Link>
  );
}

function MiniStat({ label, value, color }: { label: string; value: string; color: string }) {
  return (
    <div className="flex items-center gap-1">
      <span className="text-[8px] uppercase tracking-wider text-slate-600">{label}</span>
      <span className="text-[10px] font-mono font-bold" style={{ color }}>
        {value}
      </span>
    </div>
  );
}
