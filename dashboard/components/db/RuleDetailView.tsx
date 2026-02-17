"use client";

import { useState, useEffect } from "react";
import { RULE_KIND_META, type RuleDocument, type RuleKind } from "@/lib/api-rules";
import {
  runRuleNow,
  testNotify,
  getRuleHistory,
  type RunResult,
  type TestNotifyResult,
  type TriggerEntry,
} from "@/lib/api-anomaly-rules";
import AuditLogViewer from "@/components/db/AuditLogViewer";

// ── Types ────────────────────────────────────────────────────────────

interface RuleDetailViewProps {
  rule: RuleDocument;
  onEdit: () => void;
  onDelete: () => void;
  onToggle: () => void;
  toggling: boolean;
  deleting: boolean;
  refreshKey: number;
}

// ── Main component ───────────────────────────────────────────────────

export default function RuleDetailView({
  rule,
  onEdit,
  onDelete,
  onToggle,
  toggling,
  deleting,
  refreshKey,
}: RuleDetailViewProps) {
  const kind = rule.kind as RuleKind;
  const meta = RULE_KIND_META[kind] || { label: rule.kind, color: "#94a3b8", short: rule.kind };

  return (
    <div>
      {/* Rule header */}
      <div className="flex items-center justify-between mb-6">
        <div className="flex items-center gap-3">
          <span
            className="w-2.5 h-2.5 rounded-full"
            style={{
              background: rule.metadata.enabled ? "#06d6a0" : "#ffe600",
              boxShadow: rule.metadata.enabled
                ? "0 0 8px rgba(6, 214, 160, 0.5)"
                : "0 0 8px rgba(255, 230, 0, 0.5)",
            }}
          />
          <h2 className="text-lg font-bold font-mono" style={{ color: meta.color }}>
            {rule.metadata.name}
          </h2>
          <span
            className="px-2 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider"
            style={{
              background: `${meta.color}15`,
              border: `1px solid ${meta.color}30`,
              color: meta.color,
            }}
          >
            {meta.short}
          </span>
          <span
            className="px-2 py-0.5 rounded-full text-[9px] font-bold uppercase tracking-wider"
            style={{
              background: rule.metadata.enabled
                ? "rgba(6, 214, 160, 0.1)"
                : "rgba(255, 230, 0, 0.1)",
              border: `1px solid ${rule.metadata.enabled ? "rgba(6, 214, 160, 0.3)" : "rgba(255, 230, 0, 0.3)"}`,
              color: rule.metadata.enabled ? "#06d6a0" : "#ffe600",
            }}
          >
            {rule.metadata.enabled ? "Active" : "Paused"}
          </span>
        </div>
      </div>

      {/* Action buttons */}
      <div className="flex items-center gap-2 mb-6">
        <ActionButton label="Edit" color="#a855f7" onClick={onEdit} />
        <ActionButton
          label={rule.metadata.enabled ? "Pause" : "Start"}
          color={rule.metadata.enabled ? "#ffe600" : "#06d6a0"}
          onClick={onToggle}
          disabled={toggling}
          loading={toggling}
        />
        <ActionButton
          label="Delete"
          color="#ff4757"
          onClick={onDelete}
          disabled={deleting}
          loading={deleting}
        />
      </div>

      {/* Kind-specific content */}
      {kind === "AnomalyRule" && <AnomalyRuleDetail rule={rule} refreshKey={refreshKey} />}
      {kind === "EntitySchema" && <EntitySchemaDetail rule={rule} />}
      {kind === "FeatureConfig" && <FeatureConfigDetail rule={rule} />}
      {kind === "ScoringConfig" && <ScoringConfigDetail rule={rule} />}
      {kind === "TrendConfig" && <TrendConfigDetail rule={rule} />}
      {kind === "PatternConfig" && <PatternConfigDetail rule={rule} />}

      {/* Description & tags (all kinds) */}
      {(rule.metadata.description || (rule.metadata.tags && rule.metadata.tags.length > 0)) && (
        <div className="mt-6">
          {rule.metadata.description && (
            <p className="text-xs text-slate-400 font-mono mb-2">{rule.metadata.description}</p>
          )}
          {rule.metadata.tags && rule.metadata.tags.length > 0 && (
            <div className="flex gap-1.5 flex-wrap">
              {rule.metadata.tags.map((tag) => (
                <span
                  key={tag}
                  className="px-2 py-0.5 rounded-full text-[9px] font-mono"
                  style={{
                    background: `${meta.color}12`,
                    border: `1px solid ${meta.color}20`,
                    color: meta.color,
                  }}
                >
                  {tag}
                </span>
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── AnomalyRule detail (with lifecycle actions) ──────────────────────

function AnomalyRuleDetail({ rule, refreshKey }: { rule: RuleDocument; refreshKey: number }) {
  const [running, setRunning] = useState(false);
  const [testing, setTesting] = useState(false);
  const [runResult, setRunResult] = useState<RunResult | null>(null);
  const [testResults, setTestResults] = useState<TestNotifyResult[] | null>(null);
  const [history, setHistory] = useState<TriggerEntry[]>([]);
  const [historyLoading, setHistoryLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const ruleId = rule.metadata.id;

  useEffect(() => {
    setHistoryLoading(true);
    getRuleHistory(ruleId, 50)
      .then(setHistory)
      .catch(() => {})
      .finally(() => setHistoryLoading(false));
  }, [ruleId, refreshKey]);

  const handleRunNow = async () => {
    setRunning(true);
    setError(null);
    setRunResult(null);
    try {
      const result = await runRuleNow(ruleId);
      setRunResult(result);
      setHistoryLoading(true);
      getRuleHistory(ruleId, 50)
        .then(setHistory)
        .finally(() => setHistoryLoading(false));
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRunning(false);
    }
  };

  const handleTestNotify = async () => {
    setTesting(true);
    setError(null);
    setTestResults(null);
    try {
      const results = await testNotify(ruleId);
      setTestResults(results);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setTesting(false);
    }
  };

  return (
    <>
      {/* Anomaly-specific action buttons */}
      <div className="flex items-center gap-2 mb-6">
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
      </div>

      {error && (
        <div
          className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span className="text-xs text-red-400 font-medium">{error}</span>
        </div>
      )}

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
          {rule.detection?.template && (
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
                  {rule.detection.template}
                </span>
              </div>
              {rule.detection.params && (
                <div className="space-y-1">
                  {Object.entries(rule.detection.params).map(([k, v]) => (
                    <div key={k} className="flex items-center gap-2">
                      <span className="text-[9px] text-slate-500 font-mono">{k}:</span>
                      <span className="text-[10px] text-slate-300 font-mono">{String(v)}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
          {rule.detection?.compose != null && (
            <pre className="text-[9px] text-slate-400 font-mono overflow-x-auto">
              {JSON.stringify(rule.detection.compose, null, 2)}
            </pre>
          )}
        </DetailCard>

        {/* Schedule card */}
        {rule.schedule && (
          <DetailCard title="Schedule" color="#00f0ff">
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <span className="text-[9px] text-slate-500 font-mono">cron:</span>
                <code className="text-[10px] text-cyan-300 font-mono bg-cyan-500/5 px-1.5 py-0.5 rounded">
                  {rule.schedule.cron}
                </code>
              </div>
              {rule.schedule.timezone && (
                <div className="flex items-center gap-2">
                  <span className="text-[9px] text-slate-500 font-mono">timezone:</span>
                  <span className="text-[10px] text-slate-300 font-mono">{rule.schedule.timezone}</span>
                </div>
              )}
              {rule.schedule.cooldown && (
                <div className="flex items-center gap-2">
                  <span className="text-[9px] text-slate-500 font-mono">cooldown:</span>
                  <span className="text-[10px] text-slate-300 font-mono">{rule.schedule.cooldown}</span>
                </div>
              )}
            </div>
          </DetailCard>
        )}

        {/* Filters card */}
        {rule.filters && (
          <DetailCard title="Filters" color="#f97316">
            <div className="space-y-2">
              {rule.filters.entity_types && rule.filters.entity_types.length > 0 && (
                <div>
                  <span className="text-[9px] text-slate-500 font-mono block mb-1">entity types:</span>
                  <div className="flex gap-1 flex-wrap">
                    {rule.filters.entity_types.map((et) => (
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
              {rule.filters.min_score != null && (
                <div className="flex items-center gap-2">
                  <span className="text-[9px] text-slate-500 font-mono">min score:</span>
                  <span className="text-[10px] text-slate-300 font-mono">{rule.filters.min_score}</span>
                </div>
              )}
            </div>
          </DetailCard>
        )}

        {/* Notifications card */}
        {rule.notifications && (
          <DetailCard
            title={`Notifications (${rule.notifications.length})`}
            color="#10b981"
          >
            {rule.notifications.length === 0 ? (
              <p className="text-[10px] text-slate-600 font-mono italic">No channels configured</p>
            ) : (
              <div className="space-y-2">
                {rule.notifications.map((n, i) => (
                  <div key={i} className="flex items-center gap-2">
                    <span
                      className="px-1.5 py-0.5 rounded text-[8px] font-bold uppercase tracking-wider"
                      style={{
                        background: "rgba(16, 185, 129, 0.1)",
                        border: "1px solid rgba(16, 185, 129, 0.2)",
                        color: "#10b981",
                      }}
                    >
                      {n.channel}
                    </span>
                  </div>
                ))}
              </div>
            )}
          </DetailCard>
        )}
      </div>

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
            style={{ border: "1px solid rgba(51, 65, 85, 0.2)" }}
          >
            <table className="w-full">
              <thead>
                <tr style={{ background: "rgba(15, 23, 42, 0.8)" }}>
                  <th className="text-left px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Timestamp</th>
                  <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Matches</th>
                  <th className="text-right px-4 py-2 text-[9px] text-slate-500 uppercase tracking-wider font-bold">Duration</th>
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
                    <td className="px-4 py-2 text-[10px] text-slate-400 font-mono">{entry.timestamp}</td>
                    <td className="px-4 py-2 text-[10px] text-right font-mono" style={{ color: "#00f0ff" }}>{entry.matches_found}</td>
                    <td className="px-4 py-2 text-[10px] text-right text-slate-500 font-mono">{entry.evaluation_ms}ms</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>

      {/* Audit logs */}
      <div className="mt-6">
        <AuditLogViewer ruleId={ruleId} refreshKey={refreshKey} />
      </div>
    </>
  );
}

// ── Config kind detail views ─────────────────────────────────────────

function EntitySchemaDetail({ rule }: { rule: RuleDocument }) {
  const spec = rule.spec as Record<string, unknown> | undefined;
  if (!spec) return <SpecFallback />;

  const entityTypes = (spec.entity_types as Array<{ name: string; key_prefix: string }>) || [];
  const edgeTypes = (spec.edge_types as Array<{ name: string; from: string; to: string }>) || [];
  const fieldMappings = (spec.field_mappings as Array<{ field: string; entity_type: string }>) || [];

  return (
    <div className="grid grid-cols-2 gap-4 mb-6">
      <DetailCard title={`Entity Types (${entityTypes.length})`} color="#06b6d4">
        {entityTypes.length === 0 ? (
          <p className="text-[10px] text-slate-600 font-mono italic">None defined</p>
        ) : (
          <div className="space-y-1">
            {entityTypes.map((et) => (
              <div key={et.name} className="flex items-center gap-2">
                <span className="text-[10px] text-cyan-300 font-mono font-bold">{et.name}</span>
                <span className="text-[9px] text-slate-500 font-mono">{et.key_prefix}</span>
              </div>
            ))}
          </div>
        )}
      </DetailCard>

      <DetailCard title={`Edge Types (${edgeTypes.length})`} color="#06b6d4">
        {edgeTypes.length === 0 ? (
          <p className="text-[10px] text-slate-600 font-mono italic">None defined</p>
        ) : (
          <div className="space-y-1">
            {edgeTypes.map((e) => (
              <div key={e.name} className="flex items-center gap-1 text-[10px] font-mono">
                <span className="text-slate-400">{e.from}</span>
                <span className="text-cyan-400">→</span>
                <span className="text-slate-400">{e.to}</span>
                <span className="text-[9px] text-slate-600 ml-1">({e.name})</span>
              </div>
            ))}
          </div>
        )}
      </DetailCard>

      <DetailCard title={`Field Mappings (${fieldMappings.length})`} color="#06b6d4">
        {fieldMappings.length === 0 ? (
          <p className="text-[10px] text-slate-600 font-mono italic">None defined</p>
        ) : (
          <div className="space-y-1">
            {fieldMappings.map((fm) => (
              <div key={fm.field} className="flex items-center gap-2">
                <span className="text-[10px] text-slate-300 font-mono">{fm.field}</span>
                <span className="text-[9px] text-cyan-400 font-mono">→ {fm.entity_type}</span>
              </div>
            ))}
          </div>
        )}
      </DetailCard>

      <SpecYamlCard spec={spec} color="#06b6d4" />
    </div>
  );
}

function FeatureConfigDetail({ rule }: { rule: RuleDocument }) {
  const spec = rule.spec as Record<string, unknown> | undefined;
  if (!spec) return <SpecFallback />;

  const features = (spec.features as Array<{ name: string; index: number }>) || [];

  return (
    <div className="grid grid-cols-2 gap-4 mb-6">
      <DetailCard title={`Features (${features.length})`} color="#a855f7">
        {features.length === 0 ? (
          <p className="text-[10px] text-slate-600 font-mono italic">None defined</p>
        ) : (
          <div className="space-y-1">
            {features.map((f) => (
              <div key={f.index} className="flex items-center gap-2">
                <span className="text-[9px] text-slate-600 font-mono w-5 text-right">{f.index}</span>
                <span className="text-[10px] text-purple-300 font-mono">{f.name}</span>
              </div>
            ))}
          </div>
        )}
      </DetailCard>

      <SpecYamlCard spec={spec} color="#a855f7" />
    </div>
  );
}

function ScoringConfigDetail({ rule }: { rule: RuleDocument }) {
  const spec = rule.spec as Record<string, unknown> | undefined;
  if (!spec) return <SpecFallback />;

  const weights = spec.multi_signal_weights as Record<string, number> | undefined;
  const thresholds = spec.classification_thresholds as Record<string, number> | undefined;

  return (
    <div className="grid grid-cols-2 gap-4 mb-6">
      {weights && (
        <DetailCard title="Signal Weights" color="#10b981">
          <div className="space-y-1">
            {Object.entries(weights).map(([k, v]) => (
              <div key={k} className="flex items-center justify-between">
                <span className="text-[10px] text-slate-400 font-mono">{k}</span>
                <span className="text-[10px] text-emerald-300 font-mono font-bold">{v}</span>
              </div>
            ))}
            <div className="pt-1 border-t border-slate-700/30 flex items-center justify-between">
              <span className="text-[9px] text-slate-500 font-mono">sum</span>
              <span className="text-[9px] text-slate-400 font-mono">
                {Object.values(weights).reduce((a, b) => a + b, 0).toFixed(2)}
              </span>
            </div>
          </div>
        </DetailCard>
      )}

      {thresholds && (
        <DetailCard title="Classification Thresholds" color="#10b981">
          <div className="space-y-1">
            {Object.entries(thresholds).map(([k, v]) => (
              <div key={k} className="flex items-center justify-between">
                <span className="text-[10px] text-slate-400 font-mono">{k}</span>
                <span className="text-[10px] text-emerald-300 font-mono font-bold">{v}</span>
              </div>
            ))}
          </div>
        </DetailCard>
      )}

      <SpecYamlCard spec={spec} color="#10b981" />
    </div>
  );
}

function TrendConfigDetail({ rule }: { rule: RuleDocument }) {
  const spec = rule.spec as Record<string, unknown> | undefined;
  if (!spec) return <SpecFallback />;

  const severity = spec.severity_thresholds as Record<string, number> | undefined;

  return (
    <div className="grid grid-cols-2 gap-4 mb-6">
      <DetailCard title="Parameters" color="#3b82f6">
        <div className="space-y-1">
          {["default_window_size", "min_data_points", "z_score_trigger"].map((key) => (
            <div key={key} className="flex items-center justify-between">
              <span className="text-[10px] text-slate-400 font-mono">{key.replace(/_/g, " ")}</span>
              <span className="text-[10px] text-blue-300 font-mono font-bold">
                {spec[key] != null ? String(spec[key]) : "—"}
              </span>
            </div>
          ))}
        </div>
      </DetailCard>

      {severity && (
        <DetailCard title="Severity Thresholds" color="#3b82f6">
          <div className="space-y-1">
            {Object.entries(severity).map(([k, v]) => (
              <div key={k} className="flex items-center justify-between">
                <span className="text-[10px] text-slate-400 font-mono">{k}</span>
                <span className="text-[10px] text-blue-300 font-mono font-bold">{v}</span>
              </div>
            ))}
          </div>
        </DetailCard>
      )}

      <SpecYamlCard spec={spec} color="#3b82f6" />
    </div>
  );
}

function PatternConfigDetail({ rule }: { rule: RuleDocument }) {
  const spec = rule.spec as Record<string, unknown> | undefined;
  if (!spec) return <SpecFallback />;

  const defaults = spec.prefixspan_defaults as Record<string, unknown> | undefined;
  const classRules = (spec.classification_rules as Array<{ category: string; condition: unknown }>) || [];

  return (
    <div className="grid grid-cols-2 gap-4 mb-6">
      {defaults && (
        <DetailCard title="PrefixSpan Defaults" color="#eab308">
          <div className="space-y-1">
            {Object.entries(defaults).map(([k, v]) => (
              <div key={k} className="flex items-center justify-between">
                <span className="text-[10px] text-slate-400 font-mono">{k.replace(/_/g, " ")}</span>
                <span className="text-[10px] text-yellow-300 font-mono font-bold">{String(v)}</span>
              </div>
            ))}
          </div>
        </DetailCard>
      )}

      <DetailCard title={`Classification Rules (${classRules.length})`} color="#eab308">
        {classRules.length === 0 ? (
          <p className="text-[10px] text-slate-600 font-mono italic">None defined</p>
        ) : (
          <div className="space-y-1">
            {classRules.map((cr, i) => (
              <div key={i} className="flex items-center gap-2">
                <span className="text-[10px] text-yellow-300 font-mono font-bold">{cr.category}</span>
              </div>
            ))}
          </div>
        )}
      </DetailCard>

      <SpecYamlCard spec={spec} color="#eab308" />
    </div>
  );
}

// ── Shared sub-components ────────────────────────────────────────────

function SpecFallback() {
  return (
    <div className="rounded-lg px-4 py-6 text-center mb-6" style={{ background: "rgba(15, 23, 42, 0.5)", border: "1px solid rgba(51, 65, 85, 0.2)" }}>
      <span className="text-[10px] text-slate-600 font-mono">No spec data available</span>
    </div>
  );
}

function SpecYamlCard({ spec, color }: { spec: Record<string, unknown>; color: string }) {
  return (
    <DetailCard title="Full Spec (JSON)" color={color}>
      <pre className="text-[9px] text-slate-400 font-mono overflow-x-auto max-h-48 overflow-y-auto">
        {JSON.stringify(spec, null, 2)}
      </pre>
    </DetailCard>
  );
}

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
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
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
