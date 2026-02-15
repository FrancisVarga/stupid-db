"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import AnomalyRuleSidebar from "@/components/db/AnomalyRuleSidebar";
import AnomalyRuleForm from "@/components/db/AnomalyRuleForm";
import AuditLogViewer from "@/components/db/AuditLogViewer";
import {
  getAnomalyRule,
  deleteAnomalyRule,
  startRule,
  pauseRule,
  runRuleNow,
  testNotify,
  getRuleHistory,
  type AnomalyRule,
  type RunResult,
  type TestNotifyResult,
  type TriggerEntry,
} from "@/lib/api-anomaly-rules";

type PageMode = "view" | "add" | "edit";

export default function AnomalyRulesPage() {
  const [selectedRuleId, setSelectedRuleId] = useState<string | null>(null);
  const [selectedRule, setSelectedRule] = useState<AnomalyRule | null>(null);
  const [mode, setMode] = useState<PageMode>("view");
  const [sidebarKey, setSidebarKey] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [actionResult, setActionResult] = useState<string | null>(null);
  const [history, setHistory] = useState<TriggerEntry[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [testResults, setTestResults] = useState<TestNotifyResult[] | null>(null);
  const [runResult, setRunResult] = useState<RunResult | null>(null);
  const [toggling, setToggling] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [running, setRunning] = useState(false);
  const [testing, setTesting] = useState(false);

  const refresh = useCallback(() => {
    setSidebarKey((k) => k + 1);
  }, []);

  const loadRule = useCallback((id: string) => {
    setLoading(true);
    setError(null);
    setTestResults(null);
    setRunResult(null);
    Promise.all([getAnomalyRule(id), getRuleHistory(id, 50)])
      .then(([rule, hist]) => {
        setSelectedRule(rule);
        setHistory(hist);
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

  const handleSelectRule = useCallback(
    (id: string) => {
      setSelectedRuleId(id);
      setMode("view");
    },
    []
  );

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
    if (!selectedRule || !selectedRuleId) return;
    setToggling(true);
    setError(null);
    try {
      const fn = selectedRule.metadata.enabled ? pauseRule : startRule;
      await fn(selectedRuleId);
      refresh();
      loadRule(selectedRuleId);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setToggling(false);
    }
  };

  const handleRunNow = async () => {
    if (!selectedRuleId) return;
    setRunning(true);
    setError(null);
    setRunResult(null);
    try {
      const result = await runRuleNow(selectedRuleId);
      setRunResult(result);
      // Refresh history after run
      setHistoryLoading(true);
      getRuleHistory(selectedRuleId, 50)
        .then(setHistory)
        .finally(() => setHistoryLoading(false));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRunning(false);
    }
  };

  const handleTestNotify = async () => {
    if (!selectedRuleId) return;
    setTesting(true);
    setError(null);
    setTestResults(null);
    try {
      const results = await testNotify(selectedRuleId);
      setTestResults(results);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setTesting(false);
    }
  };

  const handleDelete = async () => {
    if (!selectedRuleId || !selectedRule) return;
    if (!confirm(`Delete rule "${selectedRule.metadata.name}"? This cannot be undone.`)) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteAnomalyRule(selectedRuleId);
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
            Anomaly Rule Manager
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
        <div style={{ width: 260 }} className="shrink-0">
          <AnomalyRuleSidebar
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

          {/* Action result banner */}
          {actionResult && (
            <div
              className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
              style={{
                background: "rgba(6, 214, 160, 0.06)",
                border: "1px solid rgba(6, 214, 160, 0.15)",
              }}
            >
              <span className="w-2 h-2 rounded-full shrink-0" style={{ background: "#06d6a0" }} />
              <span className="text-xs text-emerald-400 font-medium">{actionResult}</span>
              <button
                onClick={() => setActionResult(null)}
                className="ml-auto text-emerald-400/50 hover:text-emerald-400 text-xs"
              >
                dismiss
              </button>
            </div>
          )}

          {/* ── Form mode ────────────────────────────────── */}
          {(mode === "add" || mode === "edit") && (
            <div className="mb-6">
              <AnomalyRuleForm
                mode={mode}
                ruleId={mode === "edit" ? selectedRuleId ?? undefined : undefined}
                onSave={handleFormSave}
                onCancel={handleFormCancel}
              />
            </div>
          )}

          {/* ── View mode: nothing selected ───────────────── */}
          {mode === "view" && !selectedRuleId && (
            <div className="flex flex-col items-center justify-center py-20">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5" className="mb-4">
                <path d="M12 9v4" />
                <path d="M12 17h.01" />
                <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
              </svg>
              <p className="text-slate-500 text-sm font-mono mb-2">Select a rule or create a new one</p>
              <p className="text-slate-600 text-xs font-mono mb-4">
                Anomaly rules define detection patterns, schedules, and notification channels
              </p>
              <button
                onClick={handleNewRule}
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

          {/* ── View mode: loading selected rule ──────────── */}
          {mode === "view" && selectedRuleId && loading && (
            <div className="flex items-center justify-center py-20">
              <span className="text-slate-600 text-sm font-mono animate-pulse">Loading rule...</span>
            </div>
          )}

          {/* ── View mode: rule detail ────────────────────── */}
          {mode === "view" && selectedRule && !loading && (
            <div>
              {/* Rule header */}
              <div className="flex items-center justify-between mb-6">
                <div className="flex items-center gap-3">
                  <span
                    className="w-2.5 h-2.5 rounded-full"
                    style={{
                      background: selectedRule.metadata.enabled ? "#06d6a0" : "#ffe600",
                      boxShadow: selectedRule.metadata.enabled
                        ? "0 0 8px rgba(6, 214, 160, 0.5)"
                        : "0 0 8px rgba(255, 230, 0, 0.5)",
                    }}
                  />
                  <h2 className="text-lg font-bold font-mono" style={{ color: "#f97316" }}>
                    {selectedRule.metadata.name}
                  </h2>
                  <span
                    className="px-2 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider"
                    style={{
                      background: selectedRule.metadata.enabled
                        ? "rgba(6, 214, 160, 0.1)"
                        : "rgba(255, 230, 0, 0.1)",
                      border: `1px solid ${selectedRule.metadata.enabled ? "rgba(6, 214, 160, 0.3)" : "rgba(255, 230, 0, 0.3)"}`,
                      color: selectedRule.metadata.enabled ? "#06d6a0" : "#ffe600",
                    }}
                  >
                    {selectedRule.metadata.enabled ? "Active" : "Paused"}
                  </span>
                </div>
              </div>

              {/* Action buttons */}
              <div className="flex items-center gap-2 mb-6">
                <ActionButton
                  label="Edit"
                  color="#a855f7"
                  onClick={() => setMode("edit")}
                />
                <ActionButton
                  label={selectedRule.metadata.enabled ? "Pause" : "Start"}
                  color={selectedRule.metadata.enabled ? "#ffe600" : "#06d6a0"}
                  onClick={handleToggle}
                  disabled={toggling}
                  loading={toggling}
                />
                <ActionButton
                  label="Run Now"
                  color="#00f0ff"
                  onClick={handleRunNow}
                  disabled={running}
                  loading={running}
                />
                <ActionButton
                  label="Test Notify"
                  color="#10b981"
                  onClick={handleTestNotify}
                  disabled={testing}
                  loading={testing}
                />
                <ActionButton
                  label="Delete"
                  color="#ff4757"
                  onClick={handleDelete}
                  disabled={deleting}
                  loading={deleting}
                />
              </div>

              {/* Run result */}
              {runResult && (
                <ResultCard title="Run Result" color="#00f0ff" onDismiss={() => setRunResult(null)}>
                  <div className="grid grid-cols-3 gap-4">
                    <MiniStat label="Matches" value={runResult.matches_found} color="#00f0ff" />
                    <MiniStat label="Duration" value={`${runResult.evaluation_ms}ms`} color="#a855f7" />
                    <MiniStat label="Rule" value={runResult.rule_id} color="#f97316" />
                  </div>
                  {runResult.message && (
                    <p className="text-[10px] text-slate-500 font-mono mt-2">{runResult.message}</p>
                  )}
                </ResultCard>
              )}

              {/* Test notify results */}
              {testResults && (
                <ResultCard title="Test Notification Results" color="#10b981" onDismiss={() => setTestResults(null)}>
                  {testResults.length === 0 ? (
                    <p className="text-[10px] text-slate-500 font-mono">No notification channels configured.</p>
                  ) : (
                    <div className="space-y-2">
                      {testResults.map((tr, i) => (
                        <div
                          key={i}
                          className="flex items-center gap-3 px-3 py-2 rounded-lg"
                          style={{
                            background: tr.success ? "rgba(6, 214, 160, 0.04)" : "rgba(255, 71, 87, 0.04)",
                            border: `1px solid ${tr.success ? "rgba(6, 214, 160, 0.15)" : "rgba(255, 71, 87, 0.15)"}`,
                          }}
                        >
                          <span
                            className="w-1.5 h-1.5 rounded-full shrink-0"
                            style={{ background: tr.success ? "#06d6a0" : "#ff4757" }}
                          />
                          <span className="text-[10px] text-slate-300 font-mono flex-1">{tr.channel}</span>
                          <span className="text-[9px] text-slate-500 font-mono">{tr.response_ms}ms</span>
                          {tr.error && (
                            <span className="text-[9px] text-red-400/70 font-mono truncate max-w-[200px]">
                              {tr.error}
                            </span>
                          )}
                        </div>
                      ))}
                    </div>
                  )}
                </ResultCard>
              )}

              {/* Info cards grid */}
              <div className="grid grid-cols-2 gap-4 mb-6">
                {/* Detection card */}
                <DetailCard title="Detection" color="#a855f7">
                  {selectedRule.detection.template && (
                    <div>
                      <div className="flex items-center gap-2 mb-2">
                        <span
                          className="px-2 py-0.5 rounded text-[9px] font-bold font-mono uppercase tracking-wider"
                          style={{
                            background: "rgba(168, 85, 247, 0.1)",
                            border: "1px solid rgba(168, 85, 247, 0.2)",
                            color: "#a855f7",
                          }}
                        >
                          {selectedRule.detection.template}
                        </span>
                      </div>
                      {selectedRule.detection.params && (
                        <div className="space-y-1">
                          {Object.entries(selectedRule.detection.params).map(([k, v]) => (
                            <div key={k} className="flex items-center gap-2">
                              <span className="text-[9px] text-slate-500 font-mono">{k}:</span>
                              <span className="text-[10px] text-slate-300 font-mono">{String(v)}</span>
                            </div>
                          ))}
                        </div>
                      )}
                    </div>
                  )}
                  {selectedRule.detection.compose && (
                    <div>
                      <span
                        className="px-2 py-0.5 rounded text-[9px] font-bold font-mono uppercase tracking-wider"
                        style={{
                          background: "rgba(168, 85, 247, 0.1)",
                          border: "1px solid rgba(168, 85, 247, 0.2)",
                          color: "#a855f7",
                        }}
                      >
                        composition ({selectedRule.detection.compose.operator})
                      </span>
                      <pre className="text-[9px] text-slate-400 font-mono mt-2 overflow-x-auto">
                        {JSON.stringify(selectedRule.detection.compose, null, 2)}
                      </pre>
                    </div>
                  )}
                </DetailCard>

                {/* Schedule card */}
                <DetailCard title="Schedule" color="#00f0ff">
                  <div className="space-y-2">
                    <div className="flex items-center gap-2">
                      <span className="text-[9px] text-slate-500 font-mono">cron:</span>
                      <code className="text-[10px] text-cyan-300 font-mono bg-cyan-500/5 px-1.5 py-0.5 rounded">
                        {selectedRule.schedule.cron}
                      </code>
                    </div>
                    {selectedRule.schedule.timezone && (
                      <div className="flex items-center gap-2">
                        <span className="text-[9px] text-slate-500 font-mono">timezone:</span>
                        <span className="text-[10px] text-slate-300 font-mono">{selectedRule.schedule.timezone}</span>
                      </div>
                    )}
                    {selectedRule.schedule.cooldown && (
                      <div className="flex items-center gap-2">
                        <span className="text-[9px] text-slate-500 font-mono">cooldown:</span>
                        <span className="text-[10px] text-slate-300 font-mono">{selectedRule.schedule.cooldown}</span>
                      </div>
                    )}
                  </div>
                </DetailCard>

                {/* Filters card */}
                {selectedRule.filters && (
                  <DetailCard title="Filters" color="#f97316">
                    <div className="space-y-2">
                      {selectedRule.filters.entity_types && selectedRule.filters.entity_types.length > 0 && (
                        <div>
                          <span className="text-[9px] text-slate-500 font-mono block mb-1">entity types:</span>
                          <div className="flex gap-1 flex-wrap">
                            {selectedRule.filters.entity_types.map((et) => (
                              <span
                                key={et}
                                className="px-1.5 py-0.5 rounded text-[9px] font-mono"
                                style={{
                                  background: "rgba(249, 115, 22, 0.08)",
                                  border: "1px solid rgba(249, 115, 22, 0.2)",
                                  color: "#f97316",
                                }}
                              >
                                {et}
                              </span>
                            ))}
                          </div>
                        </div>
                      )}
                      {selectedRule.filters.min_score != null && (
                        <div className="flex items-center gap-2">
                          <span className="text-[9px] text-slate-500 font-mono">min score:</span>
                          <span className="text-[10px] text-slate-300 font-mono">{selectedRule.filters.min_score}</span>
                        </div>
                      )}
                      {selectedRule.filters.exclude_keys && selectedRule.filters.exclude_keys.length > 0 && (
                        <div>
                          <span className="text-[9px] text-slate-500 font-mono block mb-1">exclude:</span>
                          <span className="text-[9px] text-slate-400 font-mono">
                            {selectedRule.filters.exclude_keys.join(", ")}
                          </span>
                        </div>
                      )}
                    </div>
                  </DetailCard>
                )}

                {/* Notifications card */}
                <DetailCard
                  title={`Notifications (${selectedRule.notifications.length})`}
                  color="#10b981"
                >
                  {selectedRule.notifications.length === 0 ? (
                    <p className="text-[10px] text-slate-600 font-mono italic">No channels configured</p>
                  ) : (
                    <div className="space-y-2">
                      {selectedRule.notifications.map((n, i) => {
                        const channelName = n.channel === "webhook"
                          ? "Webhook"
                          : n.channel === "email"
                          ? "Email"
                          : "Telegram";
                        const detail = n.channel === "webhook"
                          ? (n.url || "")
                          : n.channel === "email"
                          ? (n.to || []).join(", ")
                          : (n.chat_id || "");

                        return (
                          <div key={i} className="flex items-center gap-2">
                            <span
                              className="px-1.5 py-0.5 rounded text-[8px] font-bold uppercase tracking-wider"
                              style={{
                                background: "rgba(16, 185, 129, 0.1)",
                                border: "1px solid rgba(16, 185, 129, 0.2)",
                                color: "#10b981",
                              }}
                            >
                              {channelName}
                            </span>
                            <span className="text-[9px] text-slate-500 font-mono truncate">
                              {detail}
                            </span>
                          </div>
                        );
                      })}
                    </div>
                  )}
                </DetailCard>
              </div>

              {/* Description & tags */}
              {(selectedRule.metadata.description || (selectedRule.metadata.tags && selectedRule.metadata.tags.length > 0)) && (
                <div className="mb-6">
                  {selectedRule.metadata.description && (
                    <p className="text-xs text-slate-400 font-mono mb-2">{selectedRule.metadata.description}</p>
                  )}
                  {selectedRule.metadata.tags && selectedRule.metadata.tags.length > 0 && (
                    <div className="flex gap-1.5 flex-wrap">
                      {selectedRule.metadata.tags.map((tag) => (
                        <span
                          key={tag}
                          className="px-2 py-0.5 rounded-full text-[9px] font-mono"
                          style={{
                            background: "rgba(249, 115, 22, 0.08)",
                            border: "1px solid rgba(249, 115, 22, 0.15)",
                            color: "#f97316",
                          }}
                        >
                          {tag}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              )}

              {/* Trigger history */}
              <div>
                <h3 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
                  Recent Triggers
                </h3>
                {historyLoading && (
                  <span className="text-[10px] text-slate-600 font-mono animate-pulse">Loading history...</span>
                )}
                {!historyLoading && history.length === 0 && (
                  <div
                    className="rounded-lg px-4 py-6 text-center"
                    style={{
                      background: "rgba(15, 23, 42, 0.5)",
                      border: "1px solid rgba(51, 65, 85, 0.2)",
                    }}
                  >
                    <span className="text-[10px] text-slate-600 font-mono">No trigger history yet</span>
                  </div>
                )}
                {!historyLoading && history.length > 0 && (
                  <div
                    className="rounded-lg overflow-hidden"
                    style={{
                      border: "1px solid rgba(51, 65, 85, 0.2)",
                    }}
                  >
                    <table className="w-full">
                      <thead>
                        <tr style={{ background: "rgba(15, 23, 42, 0.8)" }}>
                          <th className="text-left px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">
                            Timestamp
                          </th>
                          <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">
                            Matches
                          </th>
                          <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">
                            Duration
                          </th>
                        </tr>
                      </thead>
                      <tbody>
                        {history.map((entry, i) => (
                          <tr
                            key={i}
                            style={{
                              background: i % 2 === 0 ? "rgba(15, 23, 42, 0.4)" : "rgba(15, 23, 42, 0.6)",
                              borderTop: "1px solid rgba(51, 65, 85, 0.1)",
                            }}
                          >
                            <td className="px-4 py-2 text-[10px] text-slate-400 font-mono">
                              {entry.timestamp}
                            </td>
                            <td className="px-4 py-2 text-[10px] text-right font-mono" style={{ color: "#00f0ff" }}>
                              {entry.matches_found}
                            </td>
                            <td className="px-4 py-2 text-[10px] text-right text-slate-500 font-mono">
                              {entry.evaluation_ms}ms
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </div>

              {/* Audit logs */}
              <div className="mt-6">
                <AuditLogViewer ruleId={selectedRuleId!} refreshKey={sidebarKey} />
              </div>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ──────────────────────────────────────────────────

function ActionButton({
  label,
  color,
  onClick,
  disabled,
  loading: isLoading,
}: {
  label: string;
  color: string;
  onClick: () => void;
  disabled?: boolean;
  loading?: boolean;
}) {
  return (
    <button
      onClick={onClick}
      disabled={disabled}
      className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-50"
      style={{
        background: `${color}12`,
        border: `1px solid ${color}40`,
        color,
      }}
    >
      {isLoading ? "..." : label}
    </button>
  );
}

function DetailCard({
  title,
  color,
  children,
}: {
  title: string;
  color: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${color}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${color}40, transparent)` }}
      />
      <h4
        className="text-[10px] font-bold uppercase tracking-[0.15em] mb-3"
        style={{ color }}
      >
        {title}
      </h4>
      {children}
    </div>
  );
}

function ResultCard({
  title,
  color,
  onDismiss,
  children,
}: {
  title: string;
  color: string;
  onDismiss: () => void;
  children: React.ReactNode;
}) {
  return (
    <div
      className="rounded-xl p-4 mb-4 relative overflow-hidden"
      style={{
        background: `linear-gradient(135deg, #0c1018 0%, #111827 100%)`,
        border: `1px solid ${color}30`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${color}60, transparent)` }}
      />
      <div className="flex items-center justify-between mb-3">
        <h4 className="text-[10px] font-bold uppercase tracking-[0.15em]" style={{ color }}>
          {title}
        </h4>
        <button
          onClick={onDismiss}
          className="text-[10px] text-slate-600 hover:text-slate-400 transition-colors"
        >
          dismiss
        </button>
      </div>
      {children}
    </div>
  );
}

function MiniStat({
  label,
  value,
  color,
}: {
  label: string;
  value: string | number;
  color: string;
}) {
  return (
    <div>
      <div className="text-[9px] text-slate-500 uppercase tracking-wider">{label}</div>
      <div className="text-sm font-bold font-mono mt-0.5" style={{ color }}>
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}
