"use client";

import { useEffect, useState, useMemo } from "react";
import Link from "next/link";
import {
  fetchAgents,
  fetchTelemetryOverview,
  fetchGroups,
  type AgentInfo,
  type TelemetryStat,
  type AgentGroup,
} from "@/lib/api";

const TIER_CONFIG: Record<string, { color: string; label: string; order: number }> = {
  architect: { color: "#ff4757", label: "T1 Architect", order: 1 },
  lead: { color: "#a855f7", label: "T2 Lead", order: 2 },
  specialist: { color: "#06d6a0", label: "T3 Specialist", order: 3 },
};

function tierOf(tier: string) {
  return TIER_CONFIG[tier.toLowerCase()] ?? { color: "#64748b", label: tier, order: 9 };
}

export default function BundeswehrPage() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [telemetry, setTelemetry] = useState<TelemetryStat[]>([]);
  const [groups, setGroups] = useState<AgentGroup[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [groupFilter, setGroupFilter] = useState<string>("all");

  useEffect(() => {
    setLoading(true);
    Promise.all([
      fetchAgents(),
      fetchTelemetryOverview().catch(() => [] as TelemetryStat[]),
      fetchGroups().catch(() => [] as AgentGroup[]),
    ])
      .then(([a, t, g]) => {
        setAgents(a);
        setTelemetry(t);
        setGroups(g);
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load agents"))
      .finally(() => setLoading(false));
  }, []);

  const telemetryMap = useMemo(() => {
    const m = new Map<string, TelemetryStat>();
    for (const t of telemetry) m.set(t.agent_name, t);
    return m;
  }, [telemetry]);

  const groupMap = useMemo(() => {
    const m = new Map<string, string[]>();
    for (const g of groups) {
      for (const name of g.agent_names) {
        const existing = m.get(name) ?? [];
        existing.push(g.name);
        m.set(name, existing);
      }
    }
    return m;
  }, [groups]);

  const filtered = useMemo(() => {
    let list = agents;
    if (search) {
      const q = search.toLowerCase();
      list = list.filter(
        (a) => a.name.toLowerCase().includes(q) || a.description.toLowerCase().includes(q)
      );
    }
    if (groupFilter !== "all") {
      const groupAgents = groups.find((g) => g.name === groupFilter)?.agent_names ?? [];
      list = list.filter((a) => groupAgents.includes(a.name));
    }
    return list;
  }, [agents, search, groupFilter, groups]);

  const tiers = useMemo(() => {
    const map = new Map<string, AgentInfo[]>();
    for (const a of filtered) {
      const key = a.tier.toLowerCase();
      const arr = map.get(key) ?? [];
      arr.push(a);
      map.set(key, arr);
    }
    return Array.from(map.entries()).sort(
      (a, b) => tierOf(a[0]).order - tierOf(b[0]).order
    );
  }, [filtered]);

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
          <Link href="/" className="text-slate-500 hover:text-slate-300 text-xs">
            &larr; Dashboard
          </Link>
          <h1 className="text-lg font-bold tracking-wider" style={{ color: "#fbbf24" }}>
            Bundeswehr
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            Agent Management
          </span>
        </div>
        <div className="flex items-center gap-3">
          <Link
            href="/agents"
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(168, 85, 247, 0.08)",
              border: "1px solid rgba(168, 85, 247, 0.2)",
              color: "#a855f7",
            }}
          >
            Agent Chat
          </Link>
          <button
            className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
            style={{
              background: "rgba(251, 191, 36, 0.12)",
              border: "1px solid rgba(251, 191, 36, 0.3)",
              color: "#fbbf24",
            }}
          >
            + Create Agent
          </button>
        </div>
      </header>

      {/* Toolbar */}
      <div className="px-6 py-3 flex items-center gap-3">
        <input
          type="text"
          placeholder="Search agents..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="px-3 py-1.5 rounded-lg text-xs font-mono w-64 outline-none placeholder:text-slate-600"
          style={{
            background: "rgba(0, 240, 255, 0.04)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
            color: "#e2e8f0",
          }}
        />
        {groups.length > 0 && (
          <select
            value={groupFilter}
            onChange={(e) => setGroupFilter(e.target.value)}
            className="px-3 py-1.5 rounded-lg text-xs font-mono outline-none appearance-none cursor-pointer"
            style={{
              background: "rgba(0, 240, 255, 0.04)",
              border: "1px solid rgba(0, 240, 255, 0.1)",
              color: "#e2e8f0",
            }}
          >
            <option value="all">All Groups</option>
            {groups.map((g) => (
              <option key={g.name} value={g.name}>
                {g.name}
              </option>
            ))}
          </select>
        )}
        <span className="text-[10px] text-slate-600 font-mono ml-auto">
          {filtered.length} agent{filtered.length !== 1 ? "s" : ""}
        </span>
      </div>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-3">
        {loading && (
          <div className="grid grid-cols-3 gap-4">
            {Array.from({ length: 6 }).map((_, i) => (
              <div
                key={i}
                className="h-36 rounded-xl animate-pulse"
                style={{ background: "rgba(0, 240, 255, 0.03)" }}
              />
            ))}
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

        {!loading && !error && filtered.length === 0 && (
          <div className="py-12 text-center">
            <div className="text-slate-500 text-sm mb-1">No agents found</div>
            <div className="text-slate-600 text-xs font-mono">
              {search ? "Try a different search term" : "No agents configured yet"}
            </div>
          </div>
        )}

        {!loading &&
          tiers.map(([tier, tierAgents]) => {
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
      </div>
    </div>
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
