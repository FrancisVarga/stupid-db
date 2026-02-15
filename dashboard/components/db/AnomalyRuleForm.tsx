"use client";

import { useState, useEffect, useCallback } from "react";
import {
  getAnomalyRule,
  createAnomalyRule,
  updateAnomalyRule,
  type AnomalyRule,
  type NotificationChannel,
} from "@/lib/api-anomaly-rules";

// ── Types ────────────────────────────────────────────────────────────

interface AnomalyRuleFormProps {
  mode: "add" | "edit";
  ruleId?: string;
  onSave: () => void;
  onCancel: () => void;
}

type EditorTab = "visual" | "yaml";
type DetectionType = "template" | "composition";
type TemplateType = "spike" | "drift" | "absence" | "threshold";
type ChannelKind = "webhook" | "email" | "telegram";

interface FormState {
  name: string;
  description: string;
  tags: string[];
  enabled: boolean;
  detectionType: DetectionType;
  templateType: TemplateType;
  feature: string;
  templateParams: Record<string, string>;
  composeYaml: string;
  cron: string;
  timezone: string;
  cooldown: string;
  entityTypes: string[];
  minScore: string;
  excludeKeys: string;
  channels: ChannelFormEntry[];
}

interface ChannelFormEntry {
  kind: ChannelKind;
  // webhook
  url: string;
  method: string;
  // email
  to: string;
  from: string;
  smtpHost: string;
  smtpPort: string;
  // telegram
  botToken: string;
  chatId: string;
  // shared
  on: string;
  subject: string;
  body: string;
}

const ENTITY_TYPES = ["Member", "Device", "Game"];

const TEMPLATE_PARAM_KEYS: Record<TemplateType, string[]> = {
  spike: ["window", "sigma", "min_count"],
  drift: ["baseline_days", "drift_threshold"],
  absence: ["expected_interval", "grace_period"],
  threshold: ["metric", "operator", "value"],
};

const DEFAULT_FORM: FormState = {
  name: "",
  description: "",
  tags: [],
  enabled: true,
  detectionType: "template",
  templateType: "spike",
  feature: "",
  templateParams: {},
  composeYaml: "operator: and\nconditions:\n  - signal:\n      type: spike\n      feature: login_count\n      threshold: 3.0",
  cron: "0 */6 * * *",
  timezone: "UTC",
  cooldown: "1h",
  entityTypes: ["Member"],
  minScore: "",
  excludeKeys: "",
  channels: [],
};

// ── Helpers ──────────────────────────────────────────────────────────

function toKebabCase(s: string): string {
  return s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

function makeEmptyChannel(): ChannelFormEntry {
  return {
    kind: "webhook",
    url: "",
    method: "POST",
    to: "",
    from: "",
    smtpHost: "",
    smtpPort: "587",
    botToken: "",
    chatId: "",
    on: "triggered",
    subject: "",
    body: "",
  };
}

function channelEntryToNotification(ch: ChannelFormEntry): NotificationChannel {
  const base: NotificationChannel = {
    channel: ch.kind as "webhook" | "email" | "telegram",
    ...(ch.on ? { on: ch.on.split(",").map((s) => s.trim()).filter(Boolean) } : {}),
    ...(ch.subject ? { subject: ch.subject } : {}),
    ...(ch.body ? { template: ch.body } : {}),
  };
  if (ch.kind === "webhook") {
    base.url = ch.url;
    base.method = ch.method || "POST";
  } else if (ch.kind === "email") {
    base.to = ch.to.split(",").map((s) => s.trim()).filter(Boolean);
    if (ch.from) base.from = ch.from;
    if (ch.smtpHost) base.smtp_host = ch.smtpHost;
    if (ch.smtpPort) base.smtp_port = parseInt(ch.smtpPort) || 587;
  } else {
    base.bot_token = ch.botToken;
    base.chat_id = ch.chatId;
  }
  return base;
}

function notificationToChannelEntry(n: NotificationChannel): ChannelFormEntry {
  const entry = makeEmptyChannel();
  entry.kind = n.channel;
  if (n.channel === "webhook") {
    entry.url = n.url || "";
    entry.method = n.method || "POST";
  } else if (n.channel === "email") {
    entry.to = (n.to || []).join(", ");
    entry.from = n.from || "";
    entry.smtpHost = n.smtp_host || "";
    entry.smtpPort = String(n.smtp_port || 587);
  } else if (n.channel === "telegram") {
    entry.botToken = n.bot_token || "";
    entry.chatId = n.chat_id || "";
  }
  entry.on = n.on?.join(", ") || "";
  entry.subject = n.subject || "";
  entry.body = n.template || "";
  return entry;
}

function formToYaml(form: FormState, ruleId?: string): string {
  const id = ruleId || toKebabCase(form.name) || "new-rule";
  const lines: string[] = [];

  lines.push("apiVersion: stupid-db/v1");
  lines.push("kind: AnomalyRule");
  lines.push("metadata:");
  lines.push(`  id: "${id}"`);
  lines.push(`  name: "${form.name}"`);
  if (form.description) lines.push(`  description: "${form.description}"`);
  lines.push(`  enabled: ${form.enabled}`);
  if (form.tags.length > 0) {
    lines.push(`  tags: [${form.tags.map((t) => `"${t}"`).join(", ")}]`);
  }

  lines.push("schedule:");
  lines.push(`  cron: "${form.cron}"`);
  if (form.timezone) lines.push(`  timezone: "${form.timezone}"`);
  if (form.cooldown) lines.push(`  cooldown: "${form.cooldown}"`);

  lines.push("detection:");
  if (form.detectionType === "template") {
    lines.push(`  template: ${form.templateType}`);
    lines.push("  params:");
    if (form.feature) lines.push(`    feature: "${form.feature}"`);
    for (const [k, v] of Object.entries(form.templateParams)) {
      if (v) {
        const num = Number(v);
        lines.push(`    ${k}: ${isNaN(num) ? `"${v}"` : v}`);
      }
    }
  } else {
    lines.push("  compose:");
    // Indent the compose YAML under detection.compose
    for (const line of form.composeYaml.split("\n")) {
      lines.push(`    ${line}`);
    }
  }

  if (form.entityTypes.length > 0 || form.minScore || form.excludeKeys) {
    lines.push("filters:");
    if (form.entityTypes.length > 0) {
      lines.push(`  entity_types: [${form.entityTypes.map((t) => `"${t}"`).join(", ")}]`);
    }
    if (form.minScore) lines.push(`  min_score: ${form.minScore}`);
    if (form.excludeKeys.trim()) {
      const keys = form.excludeKeys
        .split("\n")
        .map((s) => s.trim())
        .filter(Boolean);
      lines.push(`  exclude_keys: [${keys.map((k) => `"${k}"`).join(", ")}]`);
    }
  }

  if (form.channels.length > 0) {
    lines.push("notifications:");
    for (const ch of form.channels) {
      const n = channelEntryToNotification(ch);
      lines.push(`  - channel: ${n.channel}`);
      if (n.on?.length) lines.push(`    on: [${n.on.map((o: string) => `"${o}"`).join(", ")}]`);
      if (n.channel === "webhook") {
        if (n.url) lines.push(`    url: "${n.url}"`);
        if (n.method) lines.push(`    method: "${n.method}"`);
      } else if (n.channel === "email") {
        if (n.to?.length) lines.push(`    to: [${n.to.map((t: string) => `"${t}"`).join(", ")}]`);
        if (n.from) lines.push(`    from: "${n.from}"`);
        if (n.smtp_host) lines.push(`    smtp_host: "${n.smtp_host}"`);
        if (n.smtp_port) lines.push(`    smtp_port: ${n.smtp_port}`);
      } else if (n.channel === "telegram") {
        if (n.bot_token) lines.push(`    bot_token: "${n.bot_token}"`);
        if (n.chat_id) lines.push(`    chat_id: "${n.chat_id}"`);
      }
      if (n.subject) lines.push(`    subject: "${n.subject}"`);
      if (n.template) lines.push(`    template: "${n.template}"`);
    }
  }

  return lines.join("\n") + "\n";
}

function ruleToFormState(rule: AnomalyRule): FormState {
  const form: FormState = { ...DEFAULT_FORM };
  form.name = rule.metadata.name;
  form.description = rule.metadata.description || "";
  form.tags = rule.metadata.tags || [];
  form.enabled = rule.metadata.enabled;
  form.cron = rule.schedule.cron;
  form.timezone = rule.schedule.timezone || "UTC";
  form.cooldown = rule.schedule.cooldown || "";

  if (rule.detection.template) {
    form.detectionType = "template";
    form.templateType = (rule.detection.template as TemplateType) || "spike";
    const params = { ...(rule.detection.params || {}) };
    form.feature = String(params.feature || "");
    delete params.feature;
    form.templateParams = Object.fromEntries(
      Object.entries(params).map(([k, v]) => [k, String(v)])
    );
  } else if (rule.detection.compose) {
    form.detectionType = "composition";
    // Best-effort serialize compose back to YAML-ish text
    form.composeYaml = JSON.stringify(rule.detection.compose, null, 2);
  }

  if (rule.filters) {
    form.entityTypes = rule.filters.entity_types || [];
    form.minScore = rule.filters.min_score != null ? String(rule.filters.min_score) : "";
    form.excludeKeys = (rule.filters.exclude_keys || []).join("\n");
  }

  form.channels = (rule.notifications || []).map(notificationToChannelEntry);

  return form;
}

// ── Component ────────────────────────────────────────────────────────

export default function AnomalyRuleForm({ mode, ruleId, onSave, onCancel }: AnomalyRuleFormProps) {
  const isEdit = mode === "edit";

  // All hooks declared before any conditional returns
  const [tab, setTab] = useState<EditorTab>("visual");
  const [form, setForm] = useState<FormState>({ ...DEFAULT_FORM });
  const [yamlText, setYamlText] = useState("");
  const [tagInput, setTagInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = useCallback(
    <K extends keyof FormState>(key: K, value: FormState[K]) => {
      setForm((prev) => ({ ...prev, [key]: value }));
    },
    []
  );

  // Load existing rule in edit mode
  useEffect(() => {
    if (!isEdit || !ruleId) return;
    let cancelled = false;
    setLoading(true);
    getAnomalyRule(ruleId)
      .then((rule) => {
        if (cancelled) return;
        setForm(ruleToFormState(rule));
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

  // Sync form -> YAML when switching to YAML tab
  useEffect(() => {
    if (tab === "yaml") {
      setYamlText(formToYaml(form, ruleId));
    }
  }, [tab]); // intentionally only on tab change

  const handleAddTag = useCallback(() => {
    const trimmed = tagInput.trim();
    if (trimmed && !form.tags.includes(trimmed)) {
      set("tags", [...form.tags, trimmed]);
    }
    setTagInput("");
  }, [tagInput, form.tags, set]);

  const handleRemoveTag = useCallback(
    (tag: string) => {
      set(
        "tags",
        form.tags.filter((t) => t !== tag)
      );
    },
    [form.tags, set]
  );

  const handleAddChannel = useCallback(() => {
    set("channels", [...form.channels, makeEmptyChannel()]);
  }, [form.channels, set]);

  const handleRemoveChannel = useCallback(
    (index: number) => {
      set(
        "channels",
        form.channels.filter((_, i) => i !== index)
      );
    },
    [form.channels, set]
  );

  const updateChannel = useCallback(
    (index: number, key: keyof ChannelFormEntry, value: string) => {
      set(
        "channels",
        form.channels.map((ch, i) => (i === index ? { ...ch, [key]: value } : ch))
      );
    },
    [form.channels, set]
  );

  const handleSave = async () => {
    setError(null);
    const yaml = tab === "yaml" ? yamlText : formToYaml(form, ruleId);

    if (tab === "visual" && !form.name.trim()) {
      setError("Rule name is required.");
      return;
    }

    setSaving(true);
    try {
      if (isEdit && ruleId) {
        await updateAnomalyRule(ruleId, yaml);
      } else {
        await createAnomalyRule(yaml);
      }
      onSave();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  // ── Styles ───────────────────────────────────────────────────────────

  const accentColor = isEdit ? "#a855f7" : "#ff8a00";
  const accentRgba = isEdit ? "rgba(168, 85, 247," : "rgba(255, 138, 0,";

  const inputClass =
    "w-full px-3 py-2 rounded-lg text-xs font-mono bg-[#0a0e15] text-slate-200 border border-slate-700/50 focus:border-cyan-500/50 focus:outline-none";
  const labelClass = "text-[10px] text-slate-500 uppercase tracking-wider block mb-1";
  const sectionClass =
    "border-t border-slate-700/30 pt-4 mt-4";

  // ── Loading state ────────────────────────────────────────────────────

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

  // ── Render ───────────────────────────────────────────────────────────

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

      {/* Header + Tab Toggle */}
      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-bold" style={{ color: accentColor }}>
          {isEdit ? "Edit Anomaly Rule" : "Add Anomaly Rule"}
        </h3>
        <div
          className="flex rounded-lg overflow-hidden"
          style={{ border: `1px solid ${accentRgba} 0.15)` }}
        >
          <button
            onClick={() => setTab("visual")}
            className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider transition-all"
            style={{
              background: tab === "visual" ? `${accentRgba} 0.12)` : "transparent",
              color: tab === "visual" ? accentColor : "#475569",
            }}
          >
            Visual Editor
          </button>
          <button
            onClick={() => setTab("yaml")}
            className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider transition-all"
            style={{
              background: tab === "yaml" ? `${accentRgba} 0.12)` : "transparent",
              color: tab === "yaml" ? accentColor : "#475569",
            }}
          >
            YAML
          </button>
        </div>
      </div>

      {/* ── YAML Tab ──────────────────────────────────────────────── */}
      {tab === "yaml" && (
        <div>
          <p className="text-[10px] text-slate-500 mb-2">
            Edit the full rule definition as YAML. Switch to Visual mode to edit fields individually.
            Note: complex rules may not fully parse back to visual mode.
          </p>
          <textarea
            value={yamlText}
            onChange={(e) => setYamlText(e.target.value)}
            rows={24}
            spellCheck={false}
            className="w-full px-4 py-3 rounded-lg text-xs font-mono bg-[#060a10] text-slate-300 border border-slate-700/50 focus:border-cyan-500/50 focus:outline-none resize-y leading-relaxed"
          />
        </div>
      )}

      {/* ── Visual Tab ────────────────────────────────────────────── */}
      {tab === "visual" && (
        <div>
          {/* ── Metadata ──────────────────────────────────────────── */}
          <div className="grid grid-cols-2 gap-3">
            <div className="col-span-2">
              <label className={labelClass}>Rule Name</label>
              <input
                type="text"
                value={form.name}
                onChange={(e) => set("name", e.target.value)}
                placeholder="e.g. High Login Spike Detection"
                className={inputClass}
              />
            </div>

            <div className="col-span-2">
              <label className={labelClass}>Description</label>
              <textarea
                value={form.description}
                onChange={(e) => set("description", e.target.value)}
                placeholder="What does this rule detect?"
                rows={2}
                className={inputClass + " resize-y"}
              />
            </div>

            {/* Tags */}
            <div className="col-span-2">
              <label className={labelClass}>Tags</label>
              <div className="flex items-center gap-2 flex-wrap">
                {form.tags.map((tag) => (
                  <span
                    key={tag}
                    className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-mono"
                    style={{
                      background: `${accentRgba} 0.1)`,
                      border: `1px solid ${accentRgba} 0.25)`,
                      color: accentColor,
                    }}
                  >
                    {tag}
                    <button
                      onClick={() => handleRemoveTag(tag)}
                      className="hover:text-white ml-0.5"
                    >
                      x
                    </button>
                  </span>
                ))}
                <input
                  type="text"
                  value={tagInput}
                  onChange={(e) => setTagInput(e.target.value)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") {
                      e.preventDefault();
                      handleAddTag();
                    }
                  }}
                  placeholder="Add tag + Enter"
                  className="px-2 py-1 rounded text-[10px] font-mono bg-[#0a0e15] text-slate-300 border border-slate-700/50 focus:border-cyan-500/50 focus:outline-none w-32"
                />
              </div>
            </div>

            {/* Enabled */}
            <div className="col-span-2 flex items-center gap-2">
              <button
                onClick={() => set("enabled", !form.enabled)}
                className="w-8 h-4 rounded-full transition-colors relative"
                style={{ background: form.enabled ? "#06d6a0" : "#1e293b" }}
              >
                <div
                  className="w-3 h-3 rounded-full bg-white absolute top-0.5 transition-all"
                  style={{ left: form.enabled ? "17px" : "2px" }}
                />
              </button>
              <span className="text-[10px] text-slate-400">Enabled</span>
            </div>
          </div>

          {/* ── Detection ─────────────────────────────────────────── */}
          <div className={sectionClass}>
            <label className={labelClass}>Detection Type</label>
            <div className="flex gap-2 mb-3">
              {(["template", "composition"] as const).map((dt) => (
                <button
                  key={dt}
                  onClick={() => set("detectionType", dt)}
                  className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all"
                  style={{
                    background:
                      form.detectionType === dt ? `${accentRgba} 0.12)` : "rgba(15, 23, 42, 0.5)",
                    border: `1px solid ${
                      form.detectionType === dt ? `${accentRgba} 0.3)` : "rgba(51, 65, 85, 0.3)"
                    }`,
                    color: form.detectionType === dt ? accentColor : "#64748b",
                  }}
                >
                  {dt === "template" ? "Template" : "Composition"}
                </button>
              ))}
            </div>

            {form.detectionType === "template" && (
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className={labelClass}>Template Type</label>
                  <select
                    value={form.templateType}
                    onChange={(e) => {
                      set("templateType", e.target.value as TemplateType);
                      set("templateParams", {});
                    }}
                    className={inputClass}
                  >
                    <option value="spike">Spike</option>
                    <option value="drift">Drift</option>
                    <option value="absence">Absence</option>
                    <option value="threshold">Threshold</option>
                  </select>
                </div>
                <div>
                  <label className={labelClass}>Feature</label>
                  <input
                    type="text"
                    value={form.feature}
                    onChange={(e) => set("feature", e.target.value)}
                    placeholder="e.g. login_count"
                    className={inputClass}
                  />
                </div>

                {/* Template-specific params */}
                {TEMPLATE_PARAM_KEYS[form.templateType].map((param) => (
                  <div key={param}>
                    <label className={labelClass}>{param.replace(/_/g, " ")}</label>
                    <input
                      type="text"
                      value={form.templateParams[param] || ""}
                      onChange={(e) =>
                        set("templateParams", { ...form.templateParams, [param]: e.target.value })
                      }
                      placeholder={param}
                      className={inputClass}
                    />
                  </div>
                ))}
              </div>
            )}

            {form.detectionType === "composition" && (
              <div>
                <label className={labelClass}>Compose Block (YAML)</label>
                <textarea
                  value={form.composeYaml}
                  onChange={(e) => set("composeYaml", e.target.value)}
                  rows={8}
                  spellCheck={false}
                  className="w-full px-4 py-3 rounded-lg text-xs font-mono bg-[#060a10] text-slate-300 border border-slate-700/50 focus:border-cyan-500/50 focus:outline-none resize-y leading-relaxed"
                />
              </div>
            )}
          </div>

          {/* ── Schedule ──────────────────────────────────────────── */}
          <div className={sectionClass}>
            <h4 className={labelClass + " mb-2"}>Schedule</h4>
            <div className="grid grid-cols-3 gap-3">
              <div>
                <label className={labelClass}>Cron</label>
                <input
                  type="text"
                  value={form.cron}
                  onChange={(e) => set("cron", e.target.value)}
                  placeholder="0 */6 * * *"
                  className={inputClass}
                />
              </div>
              <div>
                <label className={labelClass}>Timezone</label>
                <input
                  type="text"
                  value={form.timezone}
                  onChange={(e) => set("timezone", e.target.value)}
                  placeholder="UTC"
                  className={inputClass}
                />
              </div>
              <div>
                <label className={labelClass}>Cooldown</label>
                <input
                  type="text"
                  value={form.cooldown}
                  onChange={(e) => set("cooldown", e.target.value)}
                  placeholder="1h"
                  className={inputClass}
                />
              </div>
            </div>
          </div>

          {/* ── Filters ──────────────────────────────────────────── */}
          <div className={sectionClass}>
            <h4 className={labelClass + " mb-2"}>Filters</h4>
            <div className="grid grid-cols-2 gap-3">
              {/* Entity types */}
              <div className="col-span-2">
                <label className={labelClass}>Entity Types</label>
                <div className="flex gap-3">
                  {ENTITY_TYPES.map((et) => (
                    <label key={et} className="flex items-center gap-1.5 cursor-pointer">
                      <input
                        type="checkbox"
                        checked={form.entityTypes.includes(et)}
                        onChange={(e) => {
                          if (e.target.checked) {
                            set("entityTypes", [...form.entityTypes, et]);
                          } else {
                            set(
                              "entityTypes",
                              form.entityTypes.filter((t) => t !== et)
                            );
                          }
                        }}
                        className="accent-cyan-500"
                      />
                      <span className="text-[10px] text-slate-400">{et}</span>
                    </label>
                  ))}
                </div>
              </div>

              <div>
                <label className={labelClass}>Min Score</label>
                <input
                  type="number"
                  step="0.1"
                  value={form.minScore}
                  onChange={(e) => set("minScore", e.target.value)}
                  placeholder="0.0"
                  className={inputClass}
                />
              </div>

              <div>
                <label className={labelClass}>Exclude Keys (one per line)</label>
                <textarea
                  value={form.excludeKeys}
                  onChange={(e) => set("excludeKeys", e.target.value)}
                  rows={2}
                  placeholder={"system_admin\ntest_user"}
                  className={inputClass + " resize-y"}
                />
              </div>
            </div>
          </div>

          {/* ── Notifications ─────────────────────────────────────── */}
          <div className={sectionClass}>
            <div className="flex items-center justify-between mb-2">
              <h4 className={labelClass}>Notification Channels</h4>
              <button
                onClick={handleAddChannel}
                className="px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  background: `${accentRgba} 0.1)`,
                  border: `1px solid ${accentRgba} 0.3)`,
                  color: accentColor,
                }}
              >
                + Add Channel
              </button>
            </div>

            {form.channels.length === 0 && (
              <p className="text-[10px] text-slate-600 italic">No notification channels configured.</p>
            )}

            {form.channels.map((ch, idx) => (
              <div
                key={idx}
                className="rounded-lg p-3 mb-2"
                style={{
                  background: "rgba(15, 23, 42, 0.5)",
                  border: "1px solid rgba(51, 65, 85, 0.3)",
                }}
              >
                <div className="flex items-center justify-between mb-2">
                  <select
                    value={ch.kind}
                    onChange={(e) => updateChannel(idx, "kind", e.target.value)}
                    className="px-2 py-1 rounded text-[10px] font-mono bg-[#0a0e15] text-slate-300 border border-slate-700/50 focus:outline-none"
                  >
                    <option value="webhook">Webhook</option>
                    <option value="email">Email</option>
                    <option value="telegram">Telegram</option>
                  </select>
                  <button
                    onClick={() => handleRemoveChannel(idx)}
                    className="text-[10px] text-red-400/60 hover:text-red-400 transition-colors"
                  >
                    Remove
                  </button>
                </div>

                <div className="grid grid-cols-2 gap-2">
                  {ch.kind === "webhook" && (
                    <>
                      <div className="col-span-2">
                        <label className={labelClass}>URL</label>
                        <input
                          type="text"
                          value={ch.url}
                          onChange={(e) => updateChannel(idx, "url", e.target.value)}
                          placeholder="https://hooks.example.com/alert"
                          className={inputClass}
                        />
                      </div>
                      <div>
                        <label className={labelClass}>Method</label>
                        <select
                          value={ch.method}
                          onChange={(e) => updateChannel(idx, "method", e.target.value)}
                          className={inputClass}
                        >
                          <option value="POST">POST</option>
                          <option value="PUT">PUT</option>
                        </select>
                      </div>
                    </>
                  )}

                  {ch.kind === "email" && (
                    <>
                      <div className="col-span-2">
                        <label className={labelClass}>To (comma-separated)</label>
                        <input
                          type="text"
                          value={ch.to}
                          onChange={(e) => updateChannel(idx, "to", e.target.value)}
                          placeholder="alerts@example.com, ops@example.com"
                          className={inputClass}
                        />
                      </div>
                      <div>
                        <label className={labelClass}>From</label>
                        <input
                          type="text"
                          value={ch.from}
                          onChange={(e) => updateChannel(idx, "from", e.target.value)}
                          placeholder="noreply@example.com"
                          className={inputClass}
                        />
                      </div>
                      <div>
                        <label className={labelClass}>SMTP Host</label>
                        <input
                          type="text"
                          value={ch.smtpHost}
                          onChange={(e) => updateChannel(idx, "smtpHost", e.target.value)}
                          placeholder="smtp.example.com"
                          className={inputClass}
                        />
                      </div>
                      <div>
                        <label className={labelClass}>SMTP Port</label>
                        <input
                          type="text"
                          value={ch.smtpPort}
                          onChange={(e) => updateChannel(idx, "smtpPort", e.target.value)}
                          placeholder="587"
                          className={inputClass}
                        />
                      </div>
                    </>
                  )}

                  {ch.kind === "telegram" && (
                    <>
                      <div>
                        <label className={labelClass}>Bot Token</label>
                        <input
                          type="password"
                          value={ch.botToken}
                          onChange={(e) => updateChannel(idx, "botToken", e.target.value)}
                          placeholder="123456:ABC-DEF..."
                          className={inputClass}
                        />
                      </div>
                      <div>
                        <label className={labelClass}>Chat ID</label>
                        <input
                          type="text"
                          value={ch.chatId}
                          onChange={(e) => updateChannel(idx, "chatId", e.target.value)}
                          placeholder="-100123456789"
                          className={inputClass}
                        />
                      </div>
                    </>
                  )}

                  {/* Shared notification fields */}
                  <div>
                    <label className={labelClass}>On Events</label>
                    <input
                      type="text"
                      value={ch.on}
                      onChange={(e) => updateChannel(idx, "on", e.target.value)}
                      placeholder="triggered, resolved"
                      className={inputClass}
                    />
                  </div>
                  <div>
                    <label className={labelClass}>Subject</label>
                    <input
                      type="text"
                      value={ch.subject}
                      onChange={(e) => updateChannel(idx, "subject", e.target.value)}
                      placeholder="Alert: {{rule_name}}"
                      className={inputClass}
                    />
                  </div>
                </div>
              </div>
            ))}
          </div>
        </div>
      )}

      {/* ── Error ──────────────────────────────────────────────────── */}
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

      {/* ── Actions ────────────────────────────────────────────────── */}
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
