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
import AgentsTab from "@/components/bundeswehr/AgentsTab";
import OverviewTab from "@/components/bundeswehr/OverviewTab";
import SkillsTab from "@/components/bundeswehr/SkillsTab";
import PlaygroundTab from "@/components/bundeswehr/PlaygroundTab";

type Tab = "overview" | "agents" | "skills" | "prompts" | "playground";
const TABS: { key: Tab; label: string; href?: string }[] = [
  { key: "overview", label: "Overview" },
  { key: "agents", label: "Agents" },
  { key: "skills", label: "Skills" },
  { key: "prompts", label: "Prompts", href: "/bundeswehr/prompts" },
  { key: "playground", label: "Playground" },
];

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
  const [activeTab, setActiveTab] = useState<Tab>("agents");

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

      {/* Tab bar */}
      <div
        className="px-6 flex items-center gap-1"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        {TABS.map((tab) => {
          const isActive = activeTab === tab.key;
          if (tab.href) {
            return (
              <Link
                key={tab.key}
                href={tab.href}
                className="px-4 py-2.5 text-xs font-medium tracking-wide transition-colors relative"
                style={{ color: "#64748b" }}
              >
                {tab.label}
              </Link>
            );
          }
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
        {activeTab === "agents" && (
          <AgentsTab
            tiers={tiers}
            telemetryMap={telemetryMap}
            groupMap={groupMap}
            loading={loading}
            error={error}
            filtered={filtered}
            search={search}
          />
        )}
        {activeTab === "overview" && <OverviewTab />}
        {activeTab === "skills" && <SkillsTab />}
        {activeTab === "playground" && <PlaygroundTab />}
      </div>
    </div>
  );
}
