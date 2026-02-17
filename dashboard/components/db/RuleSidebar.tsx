"use client";

import { useEffect, useState, useCallback, useMemo, useRef } from "react";
import {
  listRules,
  RULE_KIND_META,
  ALL_RULE_KINDS,
  type GenericRuleSummary,
  type RuleKind,
} from "@/lib/api-rules";

type KindFilter = "all" | RuleKind;
type StatusFilter = "all" | "active" | "paused";
type SortField = "name" | "kind" | "status";
type ViewMode = "flat" | "grouped";

interface RuleSidebarProps {
  selectedRuleId: string | null;
  onSelectRule: (id: string) => void;
  onNewRule: () => void;
  refreshKey: number;
}

export default function RuleSidebar({
  selectedRuleId,
  onSelectRule,
  onNewRule,
  refreshKey,
}: RuleSidebarProps) {
  const [rules, setRules] = useState<GenericRuleSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [kindFilter, setKindFilter] = useState<KindFilter>("all");
  const [statusFilter, setStatusFilter] = useState<StatusFilter>("all");
  const [search, setSearch] = useState("");
  const [sortBy, setSortBy] = useState<SortField>("name");
  const [viewMode, setViewMode] = useState<ViewMode>("flat");
  const [collapsedKinds, setCollapsedKinds] = useState<Set<string>>(new Set());
  const searchRef = useRef<HTMLInputElement>(null);

  const loadRules = useCallback(() => {
    setLoading(true);
    setError(null);
    listRules()
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

  // Focus search on / key
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "/" && document.activeElement?.tagName !== "INPUT" && document.activeElement?.tagName !== "TEXTAREA") {
        e.preventDefault();
        searchRef.current?.focus();
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, []);

  // Filtered + sorted
  const filtered = useMemo(() => {
    let result = rules.filter((r) => {
      if (kindFilter !== "all" && r.kind !== kindFilter) return false;
      if (statusFilter === "active" && !r.enabled) return false;
      if (statusFilter === "paused" && r.enabled) return false;
      if (search) {
        const q = search.toLowerCase();
        return (
          r.name.toLowerCase().includes(q) ||
          r.id.toLowerCase().includes(q) ||
          (r.tags || []).some((t) => t.toLowerCase().includes(q))
        );
      }
      return true;
    });

    result.sort((a, b) => {
      if (sortBy === "name") return a.name.localeCompare(b.name);
      if (sortBy === "kind") return a.kind.localeCompare(b.kind) || a.name.localeCompare(b.name);
      if (sortBy === "status") {
        if (a.enabled !== b.enabled) return a.enabled ? -1 : 1;
        return a.name.localeCompare(b.name);
      }
      return 0;
    });

    return result;
  }, [rules, kindFilter, statusFilter, search, sortBy]);

  // Group by kind for grouped view
  const grouped = useMemo(() => {
    if (viewMode !== "grouped") return null;
    const groups: Record<string, GenericRuleSummary[]> = {};
    for (const r of filtered) {
      (groups[r.kind] ||= []).push(r);
    }
    return groups;
  }, [filtered, viewMode]);

  // Count by kind (unfiltered)
  const kindCounts = useMemo(() => {
    return rules.reduce(
      (acc, r) => {
        acc[r.kind] = (acc[r.kind] || 0) + 1;
        return acc;
      },
      {} as Record<string, number>
    );
  }, [rules]);

  const activeCount = rules.filter((r) => r.enabled).length;

  const toggleCollapsed = (kind: string) => {
    setCollapsedKinds((prev) => {
      const next = new Set(prev);
      if (next.has(kind)) next.delete(kind);
      else next.add(kind);
      return next;
    });
  };

  return (
    <div
      className="h-full flex flex-col overflow-hidden"
      style={{
        background: "linear-gradient(180deg, #0c1018 0%, #0a0e15 100%)",
        borderRight: "1px solid rgba(0, 240, 255, 0.08)",
      }}
    >
      {/* Header */}
      <div className="px-3 py-2 shrink-0" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-2">
            <span className="text-[10px] text-slate-500 uppercase tracking-[0.15em] font-bold">
              Rules
            </span>
            <span className="text-[9px] text-slate-600 font-mono px-1.5 py-0.5 rounded" style={{ background: "rgba(0, 240, 255, 0.04)" }}>
              {filtered.length}/{rules.length}
            </span>
          </div>
          <button
            onClick={onNewRule}
            className="flex items-center gap-1 px-2 py-0.5 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#f97316",
              border: "1px solid rgba(249, 115, 22, 0.3)",
              background: "rgba(249, 115, 22, 0.06)",
            }}
          >
            + New
          </button>
        </div>
      </div>

      {/* Search */}
      <div className="px-3 py-1.5 shrink-0" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="relative">
          <input
            ref={searchRef}
            type="text"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="Search rules..."
            className="w-full pl-2 pr-7 py-1 rounded text-[10px] font-mono text-slate-300 placeholder-slate-600 outline-none"
            style={{
              background: "rgba(255, 255, 255, 0.03)",
              border: "1px solid rgba(0, 240, 255, 0.08)",
            }}
          />
          {!search && (
            <span className="absolute right-2 top-1/2 -translate-y-1/2 text-[8px] text-slate-700 font-mono px-1 rounded" style={{ border: "1px solid rgba(71, 85, 105, 0.3)" }}>
              /
            </span>
          )}
          {search && (
            <button
              onClick={() => setSearch("")}
              className="absolute right-2 top-1/2 -translate-y-1/2 text-[10px] text-slate-600 hover:text-slate-400"
            >
              x
            </button>
          )}
        </div>
      </div>

      {/* Kind filter pills */}
      <div className="px-3 py-1.5 shrink-0 flex flex-wrap gap-0.5" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <KindPill
          label="All"
          color="#94a3b8"
          active={kindFilter === "all"}
          count={rules.length}
          onClick={() => setKindFilter("all")}
        />
        {ALL_RULE_KINDS.map((k) => (
          <KindPill
            key={k}
            label={RULE_KIND_META[k].short}
            color={RULE_KIND_META[k].color}
            active={kindFilter === k}
            count={kindCounts[k] || 0}
            onClick={() => setKindFilter(k)}
          />
        ))}
      </div>

      {/* Controls row: status filter + sort + view mode */}
      <div className="px-3 py-1 shrink-0 flex items-center gap-1.5" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        {/* Status pills */}
        <StatusPill label="All" active={statusFilter === "all"} onClick={() => setStatusFilter("all")} />
        <StatusPill label={`On ${activeCount}`} active={statusFilter === "active"} onClick={() => setStatusFilter("active")} color="#06d6a0" />
        <StatusPill label={`Off ${rules.length - activeCount}`} active={statusFilter === "paused"} onClick={() => setStatusFilter("paused")} color="#ffe600" />

        <div className="flex-1" />

        {/* Sort */}
        <select
          value={sortBy}
          onChange={(e) => setSortBy(e.target.value as SortField)}
          className="px-1 py-0.5 rounded text-[8px] font-mono text-slate-500 bg-transparent border border-slate-800 outline-none cursor-pointer"
        >
          <option value="name">A-Z</option>
          <option value="kind">Kind</option>
          <option value="status">Status</option>
        </select>

        {/* View toggle */}
        <button
          onClick={() => setViewMode((v) => (v === "flat" ? "grouped" : "flat"))}
          className="px-1 py-0.5 rounded text-[8px] font-mono transition-colors"
          style={{
            color: viewMode === "grouped" ? "#00f0ff" : "#475569",
            border: `1px solid ${viewMode === "grouped" ? "rgba(0, 240, 255, 0.2)" : "rgba(71, 85, 105, 0.3)"}`,
          }}
          title={viewMode === "flat" ? "Group by kind" : "Flat list"}
        >
          {viewMode === "flat" ? "GRP" : "LIST"}
        </button>
      </div>

      {/* Rule List */}
      <div className="flex-1 overflow-y-auto">
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

        {!loading && !error && filtered.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">
              {rules.length === 0 ? "No rules configured" : "No matching rules"}
            </span>
          </div>
        )}

        {/* Flat view */}
        {!loading && !error && viewMode === "flat" && filtered.length > 0 && (
          <div className="py-0.5">
            {filtered.map((rule) => (
              <RuleItem
                key={rule.id}
                rule={rule}
                isSelected={selectedRuleId === rule.id}
                onClick={() => onSelectRule(rule.id)}
              />
            ))}
          </div>
        )}

        {/* Grouped view */}
        {!loading && !error && viewMode === "grouped" && grouped && (
          <div className="py-0.5">
            {Object.entries(grouped).map(([kind, items]) => {
              const meta = RULE_KIND_META[kind as RuleKind];
              const isCollapsed = collapsedKinds.has(kind);

              return (
                <div key={kind}>
                  {/* Group header */}
                  <button
                    onClick={() => toggleCollapsed(kind)}
                    className="w-full px-3 py-1.5 flex items-center gap-2 sticky top-0 z-10"
                    style={{
                      background: "rgba(12, 16, 24, 0.95)",
                      borderBottom: `1px solid ${meta?.color || "#475569"}20`,
                    }}
                  >
                    <svg
                      width="8" height="8" viewBox="0 0 8 8"
                      className="shrink-0 transition-transform"
                      style={{ transform: isCollapsed ? "rotate(-90deg)" : "rotate(0)" }}
                    >
                      <path d="M2 1L6 4L2 7" fill="none" stroke={meta?.color || "#475569"} strokeWidth="1.5" />
                    </svg>
                    <span className="text-[9px] font-mono font-bold uppercase tracking-wider" style={{ color: meta?.color || "#475569" }}>
                      {meta?.label || kind}
                    </span>
                    <span className="text-[8px] text-slate-600 font-mono">
                      {items.length}
                    </span>
                  </button>

                  {/* Group items */}
                  {!isCollapsed && items.map((rule) => (
                    <RuleItem
                      key={rule.id}
                      rule={rule}
                      isSelected={selectedRuleId === rule.id}
                      onClick={() => onSelectRule(rule.id)}
                      compact
                    />
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Footer summary */}
      <div className="px-3 py-1.5 shrink-0 flex items-center justify-between" style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="text-[8px] text-slate-600 font-mono">
          {activeCount} active / {rules.length - activeCount} paused
        </div>
        <div className="text-[8px] text-slate-700 font-mono">
          {Object.entries(kindCounts)
            .filter(([, v]) => v > 0)
            .map(([k, v]) => `${v}${RULE_KIND_META[k as RuleKind]?.short?.[0] || "?"}`)
            .join(" ")}
        </div>
      </div>
    </div>
  );
}

// ── Compact rule item ────────────────────────────────────────────────

function RuleItem({
  rule,
  isSelected,
  onClick,
  compact,
}: {
  rule: GenericRuleSummary;
  isSelected: boolean;
  onClick: () => void;
  compact?: boolean;
}) {
  const meta = RULE_KIND_META[rule.kind];

  return (
    <button
      onClick={onClick}
      className="w-full text-left px-3 py-1.5 flex items-center gap-1.5 transition-all hover:bg-white/[0.02] group"
      style={{
        background: isSelected ? "rgba(0, 240, 255, 0.04)" : "transparent",
        borderLeft: isSelected ? "2px solid #00f0ff" : "2px solid transparent",
      }}
    >
      {/* Status dot */}
      <span
        className="w-1.5 h-1.5 rounded-full shrink-0"
        style={{ background: rule.enabled ? "#06d6a0" : "#ffe600" }}
      />

      {/* Name — takes remaining space */}
      <span
        className="text-[11px] font-mono truncate flex-1 leading-tight"
        style={{ color: isSelected ? "#00f0ff" : "#94a3b8" }}
      >
        {rule.name}
      </span>

      {/* Kind badge (hidden in grouped compact mode since the group header shows kind) */}
      {!compact && (
        <span
          className="text-[7px] font-mono font-bold uppercase tracking-wider px-1 py-0.5 rounded shrink-0"
          style={{
            color: meta.color,
            background: `${meta.color}12`,
            border: `1px solid ${meta.color}20`,
          }}
        >
          {meta.short.slice(0, 3)}
        </span>
      )}
    </button>
  );
}

// ── Kind filter pill ─────────────────────────────────────────────────

function KindPill({
  label,
  color,
  active,
  count,
  onClick,
}: {
  label: string;
  color: string;
  active: boolean;
  count: number;
  onClick: () => void;
}) {
  if (count === 0 && label !== "All") return null;

  return (
    <button
      onClick={onClick}
      className="px-1.5 py-0.5 rounded text-[7px] font-mono font-bold uppercase tracking-wider transition-all"
      style={{
        color: active ? color : "#475569",
        background: active ? `${color}15` : "transparent",
        border: `1px solid ${active ? `${color}30` : "rgba(71, 85, 105, 0.15)"}`,
      }}
    >
      {label}
      {count > 0 && <span className="ml-0.5 opacity-60">{count}</span>}
    </button>
  );
}

// ── Status filter pill ───────────────────────────────────────────────

function StatusPill({
  label,
  active,
  onClick,
  color,
}: {
  label: string;
  active: boolean;
  onClick: () => void;
  color?: string;
}) {
  return (
    <button
      onClick={onClick}
      className="px-1 py-0.5 rounded text-[7px] font-mono font-bold uppercase tracking-wider transition-all"
      style={{
        color: active ? (color || "#94a3b8") : "#334155",
        background: active ? `${color || "#94a3b8"}10` : "transparent",
      }}
    >
      {label}
    </button>
  );
}
