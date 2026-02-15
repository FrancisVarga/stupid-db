"use client";

import { useEffect, useState, useCallback } from "react";
import { listAnomalyRules, type RuleSummary } from "@/lib/api-anomaly-rules";

type FilterMode = "all" | "active" | "paused";

interface AnomalyRuleSidebarProps {
  selectedRuleId: string | null;
  onSelectRule: (id: string) => void;
  onNewRule: () => void;
  refreshKey: number;
}

export default function AnomalyRuleSidebar({
  selectedRuleId,
  onSelectRule,
  onNewRule,
  refreshKey,
}: AnomalyRuleSidebarProps) {
  const [rules, setRules] = useState<RuleSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [filter, setFilter] = useState<FilterMode>("all");
  const [search, setSearch] = useState("");

  const loadRules = useCallback(() => {
    setLoading(true);
    setError(null);
    listAnomalyRules()
      .then((data) => {
        setRules(data);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    loadRules();
  }, [refreshKey, loadRules]);

  const filtered = rules.filter((r) => {
    if (filter === "active" && !r.enabled) return false;
    if (filter === "paused" && r.enabled) return false;
    if (search) {
      const q = search.toLowerCase();
      return r.name.toLowerCase().includes(q) || r.id.toLowerCase().includes(q);
    }
    return true;
  });

  const activeCount = rules.filter((r) => r.enabled).length;
  const pausedCount = rules.length - activeCount;

  return (
    <div
      className="h-full flex flex-col overflow-hidden"
      style={{
        background: "linear-gradient(180deg, #0c1018 0%, #0a0e15 100%)",
        borderRight: "1px solid rgba(0, 240, 255, 0.08)",
      }}
    >
      {/* Header */}
      <div className="px-4 py-3 shrink-0" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="flex items-center justify-between">
          <div>
            <div className="text-[10px] text-slate-500 uppercase tracking-[0.15em] font-bold">
              Anomaly Rules
            </div>
            <div className="text-[9px] text-slate-600 font-mono mt-0.5">
              {rules.length} configured
            </div>
          </div>
          <button
            onClick={onNewRule}
            className="flex items-center gap-1 px-2 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#f97316",
              border: "1px solid rgba(249, 115, 22, 0.3)",
              background: "rgba(249, 115, 22, 0.06)",
            }}
          >
            <svg width="10" height="10" viewBox="0 0 24 24" fill="none" stroke="#f97316" strokeWidth="2.5" className="shrink-0">
              <line x1="12" y1="5" x2="12" y2="19" />
              <line x1="5" y1="12" x2="19" y2="12" />
            </svg>
            New Rule
          </button>
        </div>
      </div>

      {/* Search + Filter */}
      <div className="px-4 py-2 shrink-0 space-y-2" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder="Search rules..."
          className="w-full px-2 py-1 rounded text-[10px] font-mono text-slate-300 placeholder-slate-600 outline-none"
          style={{
            background: "rgba(255, 255, 255, 0.03)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        />
        <select
          value={filter}
          onChange={(e) => setFilter(e.target.value as FilterMode)}
          className="w-full px-2 py-1 rounded text-[10px] font-mono text-slate-400 outline-none cursor-pointer"
          style={{
            background: "rgba(255, 255, 255, 0.03)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <option value="all">All</option>
          <option value="active">Active</option>
          <option value="paused">Paused</option>
        </select>
      </div>

      {/* Rule List */}
      <div className="flex-1 overflow-y-auto py-2">
        {loading && (
          <div className="px-4 py-8 text-center">
            <span className="text-slate-600 text-[10px] font-mono animate-pulse">Loading...</span>
          </div>
        )}

        {error && (
          <div className="px-4 py-3">
            <span className="text-[10px] text-red-400 font-mono">{error}</span>
          </div>
        )}

        {!loading && !error && rules.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">No rules configured</span>
          </div>
        )}

        {!loading && !error && rules.length > 0 && filtered.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">No matching rules</span>
          </div>
        )}

        {filtered.map((rule) => {
          const isSelected = selectedRuleId === rule.id;

          return (
            <button
              key={rule.id}
              onClick={() => onSelectRule(rule.id)}
              className="w-full text-left px-4 py-2.5 transition-all hover:bg-white/[0.02]"
              style={{
                background: isSelected ? "rgba(0, 240, 255, 0.03)" : "transparent",
                borderLeft: isSelected ? "2px solid #00f0ff" : "2px solid transparent",
              }}
            >
              {/* Row 1: status dot + name */}
              <div className="flex items-center gap-2">
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: rule.enabled ? "#06d6a0" : "#ffe600" }}
                />
                <span
                  className="text-xs font-mono font-bold truncate"
                  style={{ color: isSelected ? "#00f0ff" : "#94a3b8" }}
                >
                  {rule.name}
                </span>
              </div>

              {/* Row 2: template + cron */}
              <div className="flex items-center gap-2 mt-1 ml-3.5">
                {rule.template && (
                  <span
                    className="text-[8px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
                    style={{
                      color: "#a855f7",
                      background: "rgba(168, 85, 247, 0.1)",
                      border: "1px solid rgba(168, 85, 247, 0.2)",
                    }}
                  >
                    {rule.template}
                  </span>
                )}
                <span className="text-[9px] text-slate-600 font-mono truncate">
                  {rule.cron}
                </span>
              </div>

              {/* Row 3: channel count */}
              <div className="flex items-center gap-1 mt-1 ml-3.5">
                <svg
                  width="9" height="9" viewBox="0 0 24 24" fill="none"
                  stroke="#475569" strokeWidth="2" className="shrink-0"
                >
                  <path d="M18 8A6 6 0 0 0 6 8c0 7-3 9-3 9h18s-3-2-3-9" />
                  <path d="M13.73 21a2 2 0 0 1-3.46 0" />
                </svg>
                <span className="text-[8px] text-slate-600 font-mono">
                  {rule.channel_count} channel{rule.channel_count !== 1 ? "s" : ""}
                </span>
              </div>
            </button>
          );
        })}
      </div>

      {/* Footer summary */}
      <div className="px-4 py-2 shrink-0" style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="text-[9px] text-slate-600 font-mono">
          {activeCount} active, {pausedCount} paused
        </div>
      </div>
    </div>
  );
}
