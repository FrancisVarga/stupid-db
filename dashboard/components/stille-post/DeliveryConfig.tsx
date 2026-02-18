"use client";

import { useState, useEffect, useCallback } from "react";

// ─── Types ──────────────────────────────────────────────────

interface Delivery {
  id: number;
  schedule_id: number;
  channel: "email" | "webhook" | "telegram";
  config_json: string;
  enabled: boolean;
}

interface Schedule {
  id: number;
  pipeline_id: number;
  name?: string;
  cron_expr?: string;
}

interface EmailConfig {
  host: string;
  port: number;
  secure: boolean;
  auth: { user: string; pass: string };
  from: string;
  to: string[];
}

interface WebhookConfig {
  url: string;
  method: string;
  headers: Record<string, string>;
}

interface TelegramConfig {
  botToken: string;
  chatId: string;
}

type ChannelType = "email" | "webhook" | "telegram";

// ─── Helpers ────────────────────────────────────────────────

const CHANNEL_ICONS: Record<ChannelType, string> = {
  email: "✉",
  webhook: "⚡",
  telegram: "✈",
};

function defaultConfig(channel: ChannelType): string {
  switch (channel) {
    case "email":
      return JSON.stringify(
        { host: "", port: 587, secure: true, auth: { user: "", pass: "" }, from: "", to: [""] },
        null,
        2,
      );
    case "webhook":
      return JSON.stringify({ url: "", method: "POST", headers: {} }, null, 2);
    case "telegram":
      return JSON.stringify({ botToken: "", chatId: "" }, null, 2);
  }
}

function parseConfig(json: string): EmailConfig | WebhookConfig | TelegramConfig | null {
  try {
    return JSON.parse(json);
  } catch {
    return null;
  }
}

function channelSummary(channel: ChannelType, configJson: string): string {
  const cfg = parseConfig(configJson);
  if (!cfg) return "invalid config";
  switch (channel) {
    case "email": {
      const e = cfg as EmailConfig;
      const recipients = e.to?.length ?? 0;
      return `${e.host || "—"}:${e.port} → ${recipients} recipient${recipients !== 1 ? "s" : ""}`;
    }
    case "webhook": {
      const w = cfg as WebhookConfig;
      return `${w.method || "POST"} ${w.url || "—"}`;
    }
    case "telegram": {
      const t = cfg as TelegramConfig;
      return `chat ${t.chatId || "—"}`;
    }
  }
}

// ─── Component ──────────────────────────────────────────────

interface DeliveryConfigProps {
  refreshKey?: number;
}

export default function DeliveryConfig({ refreshKey }: DeliveryConfigProps) {
  const [deliveries, setDeliveries] = useState<Delivery[]>([]);
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Form state
  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [formScheduleId, setFormScheduleId] = useState<number | "">("");
  const [formChannel, setFormChannel] = useState<ChannelType>("email");
  const [formConfigJson, setFormConfigJson] = useState(defaultConfig("email"));
  const [formEnabled, setFormEnabled] = useState(true);
  const [saving, setSaving] = useState(false);
  const [testingId, setTestingId] = useState<number | null>(null);
  const [testResult, setTestResult] = useState<{ id: number; ok: boolean; msg: string } | null>(null);

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [dRes, sRes] = await Promise.all([
        fetch("/api/stille-post/deliveries"),
        fetch("/api/stille-post/schedules"),
      ]);
      if (!dRes.ok) throw new Error(`Deliveries: ${dRes.status}`);
      if (!sRes.ok) throw new Error(`Schedules: ${sRes.status}`);
      const [dData, sData] = await Promise.all([dRes.json(), sRes.json()]);
      setDeliveries(Array.isArray(dData) ? dData : []);
      setSchedules(Array.isArray(sData) ? sData : []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load deliveries");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData, refreshKey]);

  // ─── Form actions ───────────────────────────────────────

  function resetForm() {
    setShowForm(false);
    setEditingId(null);
    setFormScheduleId("");
    setFormChannel("email");
    setFormConfigJson(defaultConfig("email"));
    setFormEnabled(true);
  }

  function openCreate() {
    resetForm();
    setShowForm(true);
  }

  function openEdit(d: Delivery) {
    setEditingId(d.id);
    setFormScheduleId(d.schedule_id);
    setFormChannel(d.channel);
    setFormConfigJson(
      typeof d.config_json === "string" ? d.config_json : JSON.stringify(d.config_json, null, 2),
    );
    setFormEnabled(d.enabled);
    setShowForm(true);
  }

  async function handleSave() {
    if (!formScheduleId) return;
    // Validate JSON
    try {
      JSON.parse(formConfigJson);
    } catch {
      setError("Invalid JSON in config");
      return;
    }

    setSaving(true);
    setError(null);
    try {
      const body = {
        schedule_id: Number(formScheduleId),
        channel: formChannel,
        config_json: formConfigJson,
        enabled: formEnabled,
      };

      const url = editingId
        ? `/api/stille-post/deliveries/${editingId}`
        : "/api/stille-post/deliveries";

      const res = await fetch(url, {
        method: editingId ? "PUT" : "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `${res.status}`);
      }

      resetForm();
      await fetchData();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  }

  async function handleDelete(id: number) {
    setError(null);
    try {
      const res = await fetch(`/api/stille-post/deliveries/${id}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`Delete failed: ${res.status}`);
      await fetchData();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Delete failed");
    }
  }

  async function handleToggle(d: Delivery) {
    setError(null);
    try {
      const res = await fetch(`/api/stille-post/deliveries/${d.id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...d, enabled: !d.enabled }),
      });
      if (!res.ok) throw new Error(`Toggle failed: ${res.status}`);
      await fetchData();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Toggle failed");
    }
  }

  async function handleTest(id: number) {
    setTestingId(id);
    setTestResult(null);
    try {
      const res = await fetch(`/api/stille-post/deliveries/${id}/test`, { method: "POST" });
      const ok = res.ok;
      const text = await res.text();
      setTestResult({ id, ok, msg: ok ? "Test sent successfully" : text || `Error ${res.status}` });
    } catch (e) {
      setTestResult({ id, ok: false, msg: e instanceof Error ? e.message : "Test failed" });
    } finally {
      setTestingId(null);
    }
  }

  // ─── Group deliveries by schedule ───────────────────────

  const scheduleMap = new Map(schedules.map((s) => [s.id, s]));

  const grouped = deliveries.reduce<Map<number, Delivery[]>>((acc, d) => {
    const list = acc.get(d.schedule_id) ?? [];
    list.push(d);
    acc.set(d.schedule_id, list);
    return acc;
  }, new Map());

  // ─── Render ─────────────────────────────────────────────

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div
          className="w-5 h-5 border-2 border-t-transparent rounded-full animate-spin"
          style={{ borderColor: "#00f0ff", borderTopColor: "transparent" }}
        />
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center justify-between">
        <h2 className="text-sm font-bold uppercase tracking-wider" style={{ color: "#06d6a0" }}>
          Deliveries
        </h2>
        <button
          onClick={openCreate}
          className="px-3 py-1.5 text-xs font-bold uppercase tracking-wider rounded transition-all"
          style={{
            background: "rgba(6, 214, 160, 0.12)",
            color: "#06d6a0",
            border: "1px solid rgba(6, 214, 160, 0.25)",
          }}
        >
          + Add Delivery
        </button>
      </div>

      {/* Error banner */}
      {error && (
        <div
          className="px-4 py-2 rounded text-xs font-mono"
          style={{ background: "rgba(239, 68, 68, 0.1)", border: "1px solid rgba(239, 68, 68, 0.3)", color: "#f87171" }}
        >
          {error}
        </div>
      )}

      {/* Create / Edit form */}
      {showForm && (
        <div
          className="rounded-lg p-4 space-y-3"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.15)",
          }}
        >
          <div className="text-xs font-bold uppercase tracking-wider" style={{ color: "#00f0ff" }}>
            {editingId ? "Edit Delivery" : "New Delivery"}
          </div>

          {/* Schedule selector */}
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-slate-500 mb-1">
              Schedule
            </label>
            <select
              value={formScheduleId}
              onChange={(e) => setFormScheduleId(e.target.value ? Number(e.target.value) : "")}
              className="w-full px-3 py-2 rounded text-sm font-mono"
              style={{
                background: "rgba(0, 0, 0, 0.3)",
                border: "1px solid rgba(0, 240, 255, 0.1)",
                color: "#e2e8f0",
              }}
            >
              <option value="">Select a schedule…</option>
              {schedules.map((s) => (
                <option key={s.id} value={s.id}>
                  #{s.id} — {s.name || s.cron_expr || `Schedule ${s.id}`}
                </option>
              ))}
            </select>
          </div>

          {/* Channel selector */}
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-slate-500 mb-1">
              Channel
            </label>
            <div className="flex gap-2">
              {(["email", "webhook", "telegram"] as ChannelType[]).map((ch) => {
                const active = formChannel === ch;
                return (
                  <button
                    key={ch}
                    onClick={() => {
                      setFormChannel(ch);
                      if (!editingId) setFormConfigJson(defaultConfig(ch));
                    }}
                    className="flex-1 px-3 py-2 rounded text-xs font-bold uppercase tracking-wider transition-all"
                    style={{
                      background: active ? "rgba(0, 240, 255, 0.12)" : "rgba(0, 0, 0, 0.2)",
                      color: active ? "#00f0ff" : "#475569",
                      border: `1px solid ${active ? "rgba(0, 240, 255, 0.3)" : "rgba(0, 240, 255, 0.06)"}`,
                    }}
                  >
                    {CHANNEL_ICONS[ch]} {ch}
                  </button>
                );
              })}
            </div>
          </div>

          {/* Dynamic config editor */}
          <div>
            <label className="block text-[10px] uppercase tracking-wider text-slate-500 mb-1">
              Config (JSON)
            </label>
            <ChannelConfigEditor
              channel={formChannel}
              value={formConfigJson}
              onChange={setFormConfigJson}
            />
          </div>

          {/* Enabled toggle */}
          <div className="flex items-center gap-2">
            <button
              onClick={() => setFormEnabled(!formEnabled)}
              className="w-8 h-4 rounded-full relative transition-all"
              style={{
                background: formEnabled ? "rgba(6, 214, 160, 0.4)" : "rgba(71, 85, 105, 0.3)",
              }}
            >
              <div
                className="w-3 h-3 rounded-full absolute top-0.5 transition-all"
                style={{
                  background: formEnabled ? "#06d6a0" : "#475569",
                  left: formEnabled ? "18px" : "2px",
                }}
              />
            </button>
            <span className="text-xs text-slate-400">
              {formEnabled ? "Enabled" : "Disabled"}
            </span>
          </div>

          {/* Save / Cancel */}
          <div className="flex gap-2 pt-1">
            <button
              onClick={handleSave}
              disabled={saving || !formScheduleId}
              className="px-4 py-2 rounded text-xs font-bold uppercase tracking-wider transition-all disabled:opacity-40"
              style={{
                background: "rgba(6, 214, 160, 0.15)",
                color: "#06d6a0",
                border: "1px solid rgba(6, 214, 160, 0.3)",
              }}
            >
              {saving ? "Saving…" : editingId ? "Update" : "Create"}
            </button>
            <button
              onClick={resetForm}
              className="px-4 py-2 rounded text-xs font-bold uppercase tracking-wider text-slate-500 transition-all"
              style={{ border: "1px solid rgba(71, 85, 105, 0.2)" }}
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Delivery list, grouped by schedule */}
      {deliveries.length === 0 && !showForm ? (
        <div
          className="rounded-lg p-6 text-center"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <p className="text-sm text-slate-500 font-mono">No deliveries configured yet.</p>
          <p className="text-xs text-slate-600 mt-1">
            Add a delivery to send reports via email, webhook, or telegram.
          </p>
        </div>
      ) : (
        Array.from(grouped.entries()).map(([scheduleId, items]) => {
          const sched = scheduleMap.get(scheduleId);
          return (
            <div key={scheduleId} className="space-y-2">
              <div className="text-[10px] font-bold uppercase tracking-wider text-slate-500">
                Schedule #{scheduleId}
                {sched?.name && <span className="text-slate-400 ml-1">— {sched.name}</span>}
                {sched?.cron_expr && (
                  <span className="text-slate-600 ml-1 font-mono">{sched.cron_expr}</span>
                )}
              </div>

              {items.map((d) => (
                <div
                  key={d.id}
                  className="rounded-lg p-3 flex items-center gap-3 group"
                  style={{
                    background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                    border: `1px solid ${d.enabled ? "rgba(0, 240, 255, 0.1)" : "rgba(71, 85, 105, 0.15)"}`,
                    opacity: d.enabled ? 1 : 0.6,
                  }}
                >
                  {/* Channel icon */}
                  <span className="text-lg" title={d.channel}>
                    {CHANNEL_ICONS[d.channel]}
                  </span>

                  {/* Info */}
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span
                        className="text-xs font-bold uppercase tracking-wider"
                        style={{ color: d.enabled ? "#00f0ff" : "#475569" }}
                      >
                        {d.channel}
                      </span>
                      <span className="text-[10px] text-slate-600 font-mono">#{d.id}</span>
                    </div>
                    <div className="text-xs text-slate-500 font-mono truncate">
                      {channelSummary(d.channel, d.config_json)}
                    </div>
                  </div>

                  {/* Test result */}
                  {testResult?.id === d.id && (
                    <span
                      className="text-[10px] font-mono px-2 py-0.5 rounded"
                      style={{
                        background: testResult.ok ? "rgba(6, 214, 160, 0.1)" : "rgba(239, 68, 68, 0.1)",
                        color: testResult.ok ? "#06d6a0" : "#f87171",
                      }}
                    >
                      {testResult.msg}
                    </span>
                  )}

                  {/* Actions */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity">
                    {/* Toggle */}
                    <button
                      onClick={() => handleToggle(d)}
                      className="w-8 h-4 rounded-full relative transition-all"
                      title={d.enabled ? "Disable" : "Enable"}
                      style={{
                        background: d.enabled ? "rgba(6, 214, 160, 0.4)" : "rgba(71, 85, 105, 0.3)",
                      }}
                    >
                      <div
                        className="w-3 h-3 rounded-full absolute top-0.5 transition-all"
                        style={{
                          background: d.enabled ? "#06d6a0" : "#475569",
                          left: d.enabled ? "18px" : "2px",
                        }}
                      />
                    </button>

                    {/* Test */}
                    <button
                      onClick={() => handleTest(d.id)}
                      disabled={testingId === d.id}
                      className="px-2 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-all disabled:opacity-40"
                      style={{
                        color: "#06d6a0",
                        border: "1px solid rgba(6, 214, 160, 0.2)",
                      }}
                    >
                      {testingId === d.id ? "…" : "Test"}
                    </button>

                    {/* Edit */}
                    <button
                      onClick={() => openEdit(d)}
                      className="px-2 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-all"
                      style={{
                        color: "#00f0ff",
                        border: "1px solid rgba(0, 240, 255, 0.2)",
                      }}
                    >
                      Edit
                    </button>

                    {/* Delete */}
                    <button
                      onClick={() => handleDelete(d.id)}
                      className="px-2 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-all"
                      style={{
                        color: "#f87171",
                        border: "1px solid rgba(239, 68, 68, 0.2)",
                      }}
                    >
                      Del
                    </button>
                  </div>
                </div>
              ))}
            </div>
          );
        })
      )}
    </div>
  );
}

// ─── Channel Config Editor ──────────────────────────────────
// Renders typed fields based on channel instead of raw JSON

function ChannelConfigEditor({
  channel,
  value,
  onChange,
}: {
  channel: ChannelType;
  value: string;
  onChange: (v: string) => void;
}) {
  const cfg = parseConfig(value);

  // Helper to update a nested field
  function update(path: string[], val: string | number | boolean | string[]) {
    const obj = cfg ? { ...cfg } : JSON.parse(defaultConfig(channel));
    let cursor: Record<string, unknown> = obj;
    for (let i = 0; i < path.length - 1; i++) {
      cursor = cursor[path[i]] as Record<string, unknown>;
    }
    cursor[path[path.length - 1]] = val;
    onChange(JSON.stringify(obj, null, 2));
  }

  const inputStyle = {
    background: "rgba(0, 0, 0, 0.3)",
    border: "1px solid rgba(0, 240, 255, 0.1)",
    color: "#e2e8f0",
  };

  switch (channel) {
    case "email": {
      const e = (cfg as EmailConfig) ?? { host: "", port: 587, secure: true, auth: { user: "", pass: "" }, from: "", to: [""] };
      return (
        <div className="grid grid-cols-2 gap-2">
          <Field label="SMTP Host" value={e.host} onChange={(v) => update(["host"], v)} style={inputStyle} />
          <Field label="Port" value={String(e.port)} onChange={(v) => update(["port"], Number(v) || 587)} style={inputStyle} />
          <Field label="Username" value={e.auth?.user ?? ""} onChange={(v) => update(["auth", "user"], v)} style={inputStyle} />
          <Field label="Password" value={e.auth?.pass ?? ""} onChange={(v) => update(["auth", "pass"], v)} type="password" style={inputStyle} />
          <Field label="From" value={e.from} onChange={(v) => update(["from"], v)} style={inputStyle} />
          <Field
            label="To (comma-separated)"
            value={(e.to ?? []).join(", ")}
            onChange={(v) => update(["to"], v.split(",").map((s) => s.trim()).filter(Boolean))}
            style={inputStyle}
          />
          <div className="col-span-2 flex items-center gap-2">
            <button
              onClick={() => update(["secure"], !e.secure)}
              className="w-8 h-4 rounded-full relative transition-all"
              style={{
                background: e.secure ? "rgba(6, 214, 160, 0.4)" : "rgba(71, 85, 105, 0.3)",
              }}
            >
              <div
                className="w-3 h-3 rounded-full absolute top-0.5 transition-all"
                style={{
                  background: e.secure ? "#06d6a0" : "#475569",
                  left: e.secure ? "18px" : "2px",
                }}
              />
            </button>
            <span className="text-xs text-slate-400">TLS/SSL</span>
          </div>
        </div>
      );
    }
    case "webhook": {
      const w = (cfg as WebhookConfig) ?? { url: "", method: "POST", headers: {} };
      return (
        <div className="space-y-2">
          <div className="grid grid-cols-4 gap-2">
            <div>
              <label className="block text-[10px] uppercase tracking-wider text-slate-500 mb-1">Method</label>
              <select
                value={w.method || "POST"}
                onChange={(e) => update(["method"], e.target.value)}
                className="w-full px-3 py-2 rounded text-sm font-mono"
                style={inputStyle}
              >
                <option value="POST">POST</option>
                <option value="PUT">PUT</option>
                <option value="PATCH">PATCH</option>
              </select>
            </div>
            <div className="col-span-3">
              <Field label="URL" value={w.url} onChange={(v) => update(["url"], v)} style={inputStyle} />
            </div>
          </div>
          <Field
            label="Headers (JSON)"
            value={JSON.stringify(w.headers ?? {}, null, 2)}
            onChange={(v) => {
              try {
                update(["headers"], JSON.parse(v));
              } catch {
                // let user keep typing
              }
            }}
            multiline
            style={inputStyle}
          />
        </div>
      );
    }
    case "telegram": {
      const t = (cfg as TelegramConfig) ?? { botToken: "", chatId: "" };
      return (
        <div className="grid grid-cols-2 gap-2">
          <Field label="Bot Token" value={t.botToken} onChange={(v) => update(["botToken"], v)} type="password" style={inputStyle} />
          <Field label="Chat ID" value={t.chatId} onChange={(v) => update(["chatId"], v)} style={inputStyle} />
        </div>
      );
    }
  }
}

// ─── Reusable field ─────────────────────────────────────────

function Field({
  label,
  value,
  onChange,
  type = "text",
  multiline = false,
  style,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
  type?: string;
  multiline?: boolean;
  style: Record<string, string>;
}) {
  return (
    <div>
      <label className="block text-[10px] uppercase tracking-wider text-slate-500 mb-1">{label}</label>
      {multiline ? (
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          rows={3}
          className="w-full px-3 py-2 rounded text-sm font-mono resize-y"
          style={style}
        />
      ) : (
        <input
          type={type}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          className="w-full px-3 py-2 rounded text-sm font-mono"
          style={style}
        />
      )}
    </div>
  );
}
