"use client";

import { useEffect, useState, useCallback, useMemo } from "react";
import Link from "next/link";
import RuleSidebar from "@/components/db/RuleSidebar";
import RuleForm from "@/components/db/RuleForm";
import RuleDetailView from "@/components/db/RuleDetailView";
import {
  getRule,
  deleteRule,
  toggleRule,
  listRules,
  getRecentTriggers,
  RULE_KIND_META,
  ALL_RULE_KINDS,
  type RuleDocument,
  type RuleKind,
  type GenericRuleSummary,
  type RecentTrigger,
} from "@/lib/api-rules";

type PageMode = "view" | "add" | "edit";

export default function RulesPage() {
  const [selectedRuleId, setSelectedRuleId] = useState<string | null>(null);
  const [selectedRule, setSelectedRule] = useState<RuleDocument | null>(null);
  const [mode, setMode] = useState<PageMode>("view");
  const [sidebarKey, setSidebarKey] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const refresh = useCallback(() => {
    setSidebarKey((k) => k + 1);
  }, []);

  const loadRule = useCallback((id: string) => {
    setLoading(true);
    setError(null);
    getRule(id)
      .then((rule) => {
        setSelectedRule(rule);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  // Load rule detail when selected
  useEffect(() => {
    if (selectedRuleId && mode === "view") {
      loadRule(selectedRuleId);
    }
  }, [selectedRuleId, mode, loadRule]);

  const handleSelectRule = useCallback((id: string) => {
    setSelectedRuleId(id);
    setMode("view");
  }, []);

  const handleNewRule = useCallback(() => {
    setSelectedRuleId(null);
    setSelectedRule(null);
    setMode("add");
  }, []);

  const handleFormSave = useCallback(() => {
    setMode("view");
    refresh();
    if (selectedRuleId) {
      loadRule(selectedRuleId);
    }
  }, [selectedRuleId, refresh, loadRule]);

  const handleFormCancel = useCallback(() => {
    setMode("view");
  }, []);

  const handleToggle = async () => {
    if (!selectedRuleId) return;
    setToggling(true);
    setError(null);
    try {
      await toggleRule(selectedRuleId);
      refresh();
      loadRule(selectedRuleId);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setToggling(false);
    }
  };

  const handleDelete = async () => {
    if (!selectedRuleId || !selectedRule) return;
    if (!confirm(`Delete rule "${selectedRule.metadata.name}"? This cannot be undone.`)) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteRule(selectedRuleId);
      setSelectedRuleId(null);
      setSelectedRule(null);
      setMode("view");
      refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(249, 115, 22, 0.08)",
          background: "linear-gradient(180deg, rgba(249, 115, 22, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div className="w-[1px] h-4" style={{ background: "rgba(249, 115, 22, 0.12)" }} />
          <h1 className="text-lg font-bold tracking-wider" style={{ color: "#f97316" }}>
            Rule Manager
          </h1>
        </div>
        <button
          onClick={handleNewRule}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            background: "rgba(249, 115, 22, 0.1)",
            border: "1px solid rgba(249, 115, 22, 0.3)",
            color: "#f97316",
          }}
        >
          + New Rule
        </button>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 280 }} className="shrink-0">
          <RuleSidebar
            selectedRuleId={selectedRuleId}
            onSelectRule={handleSelectRule}
            onNewRule={handleNewRule}
            refreshKey={sidebarKey}
          />
        </div>

        <div className="flex-1 overflow-y-auto px-8 py-6">
          {/* Error banner */}
          {error && (
            <div
              className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
              }}
            >
              <span className="w-2 h-2 rounded-full shrink-0 animate-pulse" style={{ background: "#ff4757" }} />
              <span className="text-xs text-red-400 font-medium">{error}</span>
              <button
                onClick={() => setError(null)}
                className="ml-auto text-red-400/50 hover:text-red-400 text-xs"
              >
                dismiss
              </button>
            </div>
          )}

          {/* Form mode */}
          {(mode === "add" || mode === "edit") && (
            <div className="mb-6">
              <RuleForm
                mode={mode}
                ruleId={mode === "edit" ? selectedRuleId ?? undefined : undefined}
                initialKind={selectedRule?.kind as RuleKind | undefined}
                onSave={handleFormSave}
                onCancel={handleFormCancel}
              />
            </div>
          )}

          {/* View mode: nothing selected — show dashboard */}
          {mode === "view" && !selectedRuleId && (
            <RuleDashboard
              onSelectRule={handleSelectRule}
              onNewRule={handleNewRule}
              refreshKey={sidebarKey}
            />
          )}

          {/* View mode: loading selected rule */}
          {mode === "view" && selectedRuleId && loading && (
            <div className="flex items-center justify-center py-20">
              <span className="text-slate-600 text-sm font-mono animate-pulse">Loading rule...</span>
            </div>
          )}

          {/* View mode: rule detail */}
          {mode === "view" && selectedRule && !loading && (
            <RuleDetailView
              rule={selectedRule}
              onEdit={() => setMode("edit")}
              onDelete={handleDelete}
              onToggle={handleToggle}
              toggling={toggling}
              deleting={deleting}
              refreshKey={sidebarKey}
            />
          )}
        </div>
      </div>
    </div>
  );
}

// ── Dashboard component (shown when no rule selected) ───────────────

function RuleDashboard({
  onSelectRule,
  onNewRule,
  refreshKey,
}: {
  onSelectRule: (id: string) => void;
  onNewRule: () => void;
  refreshKey: number;
}) {
  const [rules, setRules] = useState<GenericRuleSummary[]>([]);
  const [triggers, setTriggers] = useState<RecentTrigger[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    Promise.all([listRules(), getRecentTriggers(50)])
      .then(([r, t]) => {
        setRules(r);
        setTriggers(t);
      })
      .catch(() => {})
      .finally(() => setLoading(false));
  }, [refreshKey]);

  // Kind distribution
  const kindCounts = useMemo(() => {
    const counts: Record<string, number> = {};
    for (const r of rules) {
      counts[r.kind] = (counts[r.kind] || 0) + 1;
    }
    return counts;
  }, [rules]);

  const activeCount = rules.filter((r) => r.enabled).length;

  if (loading) {
    return (
      <div className="flex items-center justify-center py-20">
        <span className="text-slate-600 text-sm font-mono animate-pulse">Loading dashboard...</span>
      </div>
    );
  }

  return (
    <div>
      {/* Overview stats */}
      <div className="grid grid-cols-4 gap-4 mb-6">
        <DashStat label="Total Rules" value={rules.length} color="#f97316" />
        <DashStat label="Active" value={activeCount} color="#06d6a0" />
        <DashStat label="Paused" value={rules.length - activeCount} color="#ffe600" />
        <DashStat label="Recent Triggers" value={triggers.length} color="#00f0ff" />
      </div>

      {/* Kind breakdown */}
      <div className="mb-6">
        <h3 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
          Rules by Kind
        </h3>
        <div className="flex gap-2 flex-wrap">
          {ALL_RULE_KINDS.map((k) => {
            const count = kindCounts[k] || 0;
            const meta = RULE_KIND_META[k];
            return (
              <div
                key={k}
                className="flex items-center gap-2 px-3 py-2 rounded-lg"
                style={{
                  background: count > 0 ? `${meta.color}08` : "rgba(15, 23, 42, 0.3)",
                  border: `1px solid ${count > 0 ? `${meta.color}20` : "rgba(51, 65, 85, 0.15)"}`,
                }}
              >
                <span
                  className="w-2 h-2 rounded-full"
                  style={{ background: count > 0 ? meta.color : "#334155" }}
                />
                <span className="text-[10px] font-mono" style={{ color: count > 0 ? meta.color : "#475569" }}>
                  {meta.short}
                </span>
                <span className="text-[11px] font-mono font-bold" style={{ color: count > 0 ? meta.color : "#334155" }}>
                  {count}
                </span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Recent trigger feed */}
      <div className="mb-6">
        <h3 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
          Recent Trigger Activity
        </h3>
        {triggers.length === 0 ? (
          <div
            className="rounded-lg px-4 py-8 text-center"
            style={{
              background: "rgba(15, 23, 42, 0.5)",
              border: "1px solid rgba(51, 65, 85, 0.2)",
            }}
          >
            <svg
              width="32" height="32" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5"
              className="mx-auto mb-3"
            >
              <circle cx="12" cy="12" r="10" />
              <path d="M12 6v6l4 2" />
            </svg>
            <p className="text-[10px] text-slate-600 font-mono mb-1">No trigger history yet</p>
            <p className="text-[9px] text-slate-700 font-mono">
              Triggers appear here when rules are evaluated via schedule or manual run
            </p>
          </div>
        ) : (
          <div
            className="rounded-lg overflow-hidden"
            style={{ border: "1px solid rgba(51, 65, 85, 0.2)" }}
          >
            <table className="w-full">
              <thead>
                <tr style={{ background: "rgba(15, 23, 42, 0.8)" }}>
                  <th className="text-left px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Time</th>
                  <th className="text-left px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Rule</th>
                  <th className="text-left px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Kind</th>
                  <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Matches</th>
                  <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Duration</th>
                </tr>
              </thead>
              <tbody>
                {triggers.map((t, i) => {
                  const meta = RULE_KIND_META[t.kind] || { color: "#94a3b8", short: t.kind };
                  return (
                    <tr
                      key={i}
                      onClick={() => onSelectRule(t.rule_id)}
                      className="cursor-pointer hover:bg-white/[0.03] transition-colors"
                      style={{
                        background: i % 2 === 0 ? "rgba(15, 23, 42, 0.4)" : "rgba(15, 23, 42, 0.6)",
                        borderTop: "1px solid rgba(51, 65, 85, 0.1)",
                      }}
                    >
                      <td className="px-4 py-2 text-[10px] text-slate-500 font-mono">
                        {formatTimestamp(t.timestamp)}
                      </td>
                      <td className="px-4 py-2 text-[11px] text-slate-300 font-mono truncate max-w-[200px]">
                        {t.rule_name}
                      </td>
                      <td className="px-4 py-2">
                        <span
                          className="px-1.5 py-0.5 rounded text-[7px] font-mono font-bold uppercase tracking-wider"
                          style={{
                            color: meta.color,
                            background: `${meta.color}12`,
                            border: `1px solid ${meta.color}20`,
                          }}
                        >
                          {meta.short.slice(0, 3)}
                        </span>
                      </td>
                      <td
                        className="px-4 py-2 text-[10px] text-right font-mono font-bold"
                        style={{ color: t.matches_found > 0 ? "#00f0ff" : "#475569" }}
                      >
                        {t.matches_found}
                      </td>
                      <td className="px-4 py-2 text-[10px] text-right text-slate-500 font-mono">
                        {t.evaluation_ms}ms
                      </td>
                    </tr>
                  );
                })}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Quick action */}
      {rules.length === 0 && (
        <div className="text-center">
          <button
            onClick={onNewRule}
            className="px-4 py-2 rounded-lg text-xs font-bold uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              background: "rgba(249, 115, 22, 0.1)",
              border: "1px solid rgba(249, 115, 22, 0.3)",
              color: "#f97316",
            }}
          >
            + Create Your First Rule
          </button>
        </div>
      )}
    </div>
  );
}

function DashStat({ label, value, color }: { label: string; value: number; color: string }) {
  return (
    <div
      className="rounded-lg px-4 py-3 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${color}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${color}40, transparent)` }}
      />
      <div className="text-[9px] text-slate-500 uppercase tracking-wider font-bold">{label}</div>
      <div className="text-xl font-bold font-mono mt-1" style={{ color }}>
        {value}
      </div>
    </div>
  );
}

function formatTimestamp(iso: string): string {
  try {
    const d = new Date(iso);
    const now = new Date();
    const diffMs = now.getTime() - d.getTime();
    const diffMin = Math.floor(diffMs / 60000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    const diffDay = Math.floor(diffHr / 24);
    if (diffDay < 7) return `${diffDay}d ago`;
    return d.toLocaleDateString("en-US", { month: "short", day: "numeric" });
  } catch {
    return iso;
  }
}
