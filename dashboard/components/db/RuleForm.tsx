"use client";

import { useState, useEffect, useCallback } from "react";
import {
  getRuleYaml,
  createRule,
  updateRule,
  RULE_KIND_META,
  ALL_RULE_KINDS,
  type RuleKind,
} from "@/lib/api-rules";
import CodeEditor from "@/components/db/CodeEditor";

// ── Types ────────────────────────────────────────────────────────────

interface RuleFormProps {
  mode: "add" | "edit";
  ruleId?: string;
  initialKind?: RuleKind;
  onSave: () => void;
  onCancel: () => void;
}

// ── Template YAML per kind ───────────────────────────────────────────

const RULE_TEMPLATES: Record<RuleKind, string> = {
  AnomalyRule: `apiVersion: v1
kind: AnomalyRule
metadata:
  id: new-anomaly-rule
  name: New Anomaly Rule
  description: ""
  enabled: true
  tags: []
schedule:
  cron: "0 */6 * * *"
  timezone: UTC
  cooldown: "1h"
detection:
  template: spike
  params:
    feature: login_count
    window: "24h"
    sigma: 3.0
filters:
  entity_types: ["Member"]
notifications: []
`,
  EntitySchema: `apiVersion: v1
kind: EntitySchema
metadata:
  id: new-entity-schema
  name: New Entity Schema
  description: ""
  enabled: true
spec:
  null_values: ["", "N/A", "null"]
  entity_types:
    - name: Member
      key_prefix: "member:"
  edge_types: []
  field_mappings: []
  event_extraction: {}
  embedding_templates: {}
`,
  FeatureConfig: `apiVersion: v1
kind: FeatureConfig
metadata:
  id: new-feature-config
  name: New Feature Config
  description: ""
  enabled: true
spec:
  features:
    - name: login_count
      index: 0
  vip_encoding: {}
  vip_fallback: zero
  currency_encoding: {}
  currency_fallback: zero
  event_classification: {}
  mobile_keywords: []
  event_compression: {}
`,
  ScoringConfig: `apiVersion: v1
kind: ScoringConfig
metadata:
  id: new-scoring-config
  name: New Scoring Config
  description: ""
  enabled: true
spec:
  multi_signal_weights:
    statistical: 0.35
    dbscan_noise: 0.25
    behavioral: 0.25
    graph: 0.15
  classification_thresholds:
    mild: 0.3
    anomalous: 0.6
    highly_anomalous: 0.85
  z_score_normalization:
    cap: 10.0
    floor: 0.0
  graph_anomaly:
    degree_weight: 0.4
    pagerank_weight: 0.3
    community_weight: 0.3
  default_anomaly_threshold: 2.0
`,
  TrendConfig: `apiVersion: v1
kind: TrendConfig
metadata:
  id: new-trend-config
  name: New Trend Config
  description: ""
  enabled: true
spec:
  default_window_size: 30
  min_data_points: 5
  z_score_trigger: 2.0
  direction_thresholds:
    up: 1.0
    down: 1.0
  severity_thresholds:
    notable: 2.0
    significant: 3.0
    critical: 4.0
`,
  PatternConfig: `apiVersion: v1
kind: PatternConfig
metadata:
  id: new-pattern-config
  name: New Pattern Config
  description: ""
  enabled: true
spec:
  prefixspan_defaults:
    min_support: 0.1
    max_length: 5
    min_members: 3
  classification_rules: []
`,
};

// ── Component ────────────────────────────────────────────────────────

export default function RuleForm({ mode, ruleId, initialKind, onSave, onCancel }: RuleFormProps) {
  const isEdit = mode === "edit";

  const [selectedKind, setSelectedKind] = useState<RuleKind>(initialKind || "AnomalyRule");
  const [yamlText, setYamlText] = useState(RULE_TEMPLATES[initialKind || "AnomalyRule"]);
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Load existing rule YAML in edit mode
  useEffect(() => {
    if (!isEdit || !ruleId) return;
    let cancelled = false;
    setLoading(true);
    getRuleYaml(ruleId)
      .then((yaml) => {
        if (cancelled) return;
        setYamlText(yaml);
      })
      .catch((e) => {
        if (!cancelled) setError((e as Error).message);
      })
      .finally(() => {
        if (!cancelled) setLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [isEdit, ruleId]);

  // Update template when kind changes (only in add mode)
  const handleKindChange = useCallback((kind: RuleKind) => {
    setSelectedKind(kind);
    setYamlText(RULE_TEMPLATES[kind]);
  }, []);

  const handleSave = async () => {
    setError(null);
    if (!yamlText.trim()) {
      setError("YAML content is required.");
      return;
    }

    setSaving(true);
    try {
      if (isEdit && ruleId) {
        await updateRule(ruleId, yamlText);
      } else {
        await createRule(yamlText);
      }
      onSave();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const accentColor = isEdit ? "#a855f7" : "#ff8a00";
  const accentRgba = isEdit ? "rgba(168, 85, 247," : "rgba(255, 138, 0,";

  if (loading) {
    return (
      <div
        className="rounded-xl p-6 text-center text-xs text-slate-500"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: `1px solid ${accentRgba} 0.2)`,
        }}
      >
        Loading rule...
      </div>
    );
  }

  return (
    <div
      className="rounded-xl p-6 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accentRgba} 0.2)`,
      }}
    >
      {/* Top accent line */}
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accentRgba} 0.4), transparent)`,
        }}
      />

      {/* Header */}
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-bold" style={{ color: accentColor }}>
          {isEdit ? "Edit Rule" : "Add Rule"}
        </h3>
      </div>

      {/* Kind selector (only in add mode) */}
      {!isEdit && (
        <div className="mb-4">
          <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-2">
            Rule Kind
          </label>
          <div className="flex gap-1.5 flex-wrap">
            {ALL_RULE_KINDS.map((k) => {
              const meta = RULE_KIND_META[k];
              const isActive = selectedKind === k;
              return (
                <button
                  key={k}
                  onClick={() => handleKindChange(k)}
                  className="px-2.5 py-1 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all"
                  style={{
                    background: isActive ? `${meta.color}15` : "rgba(15, 23, 42, 0.5)",
                    border: `1px solid ${isActive ? `${meta.color}40` : "rgba(51, 65, 85, 0.3)"}`,
                    color: isActive ? meta.color : "#64748b",
                  }}
                >
                  {meta.short}
                </button>
              );
            })}
          </div>
        </div>
      )}

      {/* YAML editor */}
      <div>
        <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
          Rule Definition (YAML)
        </label>
        <p className="text-[10px] text-slate-600 mb-2">
          Edit the full rule definition as YAML. The server validates the document on save.
        </p>
        <CodeEditor
          value={yamlText}
          onChange={setYamlText}
          language="yaml"
          minHeight="300px"
          maxHeight="600px"
        />
      </div>

      {/* Error */}
      {error && (
        <div
          className="mt-3 px-3 py-2 rounded-lg text-[10px] font-mono"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.2)",
            color: "#ff4757",
          }}
        >
          {error}
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center gap-2 mt-4">
        <button
          onClick={handleSave}
          disabled={saving}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-50"
          style={{
            background: `${accentRgba} 0.1)`,
            border: `1px solid ${accentRgba} 0.3)`,
            color: accentColor,
          }}
        >
          {saving ? "Saving..." : isEdit ? "Update Rule" : "Create Rule"}
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider text-slate-500 hover:text-slate-300 transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
