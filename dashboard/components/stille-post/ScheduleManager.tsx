"use client";

import { useState, useEffect, useCallback } from "react";

/* ── Types ─────────────────────────────────────────────── */

interface Schedule {
  id: number;
  pipeline_id: number;
  cron_expression: string;
  timezone: string;
  enabled: boolean;
  last_run_at: string | null;
  next_run_at: string | null;
  created_at: string;
}

interface Pipeline {
  id: number;
  name: string;
}

interface ScheduleForm {
  pipeline_id: number | "";
  cron_expression: string;
  timezone: string;
  enabled: boolean;
}

/* ── Cron helpers ──────────────────────────────────────── */

const CRON_PRESETS: { label: string; cron: string }[] = [
  { label: "Every minute", cron: "* * * * *" },
  { label: "Every 5 minutes", cron: "*/5 * * * *" },
  { label: "Every hour", cron: "0 * * * *" },
  { label: "Every day at midnight", cron: "0 0 * * *" },
  { label: "Every day at 8:00 AM", cron: "0 8 * * *" },
  { label: "Every Monday at 9:00 AM", cron: "0 9 * * 1" },
  { label: "Every 1st of month", cron: "0 0 1 * *" },
];

const COMMON_TIMEZONES = [
  "UTC",
  "Europe/Berlin",
  "Europe/London",
  "America/New_York",
  "America/Chicago",
  "America/Denver",
  "America/Los_Angeles",
  "Asia/Tokyo",
  "Asia/Shanghai",
  "Australia/Sydney",
];

/** Parse a cron expression into a human-readable string. */
function describeCron(expr: string): string {
  const parts = expr.trim().split(/\s+/);
  if (parts.length !== 5) return "Invalid cron expression";
  const [min, hour, dom, mon, dow] = parts;

  // Every minute
  if (min === "*" && hour === "*" && dom === "*" && mon === "*" && dow === "*")
    return "Every minute";

  // */N minutes
  if (min.startsWith("*/") && hour === "*" && dom === "*" && mon === "*" && dow === "*")
    return `Every ${min.slice(2)} minutes`;

  // Top of every hour
  if (min === "0" && hour === "*" && dom === "*" && mon === "*" && dow === "*")
    return "Every hour";

  // Specific minute every hour
  if (/^\d+$/.test(min) && hour === "*" && dom === "*" && mon === "*" && dow === "*")
    return `Every hour at :${min.padStart(2, "0")}`;

  const dayNames = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];
  const monthNames = [
    "", "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
  ];

  const timeStr =
    /^\d+$/.test(hour) && /^\d+$/.test(min)
      ? `${hour.padStart(2, "0")}:${min.padStart(2, "0")}`
      : null;

  // Daily at specific time
  if (timeStr && dom === "*" && mon === "*" && dow === "*")
    return `Every day at ${timeStr}`;

  // Weekly on specific day
  if (timeStr && dom === "*" && mon === "*" && /^\d$/.test(dow))
    return `Every ${dayNames[parseInt(dow)] ?? dow} at ${timeStr}`;

  // Monthly on specific day
  if (timeStr && /^\d+$/.test(dom) && mon === "*" && dow === "*") {
    const suffix = dom === "1" ? "st" : dom === "2" ? "nd" : dom === "3" ? "rd" : "th";
    return `${dom}${suffix} of every month at ${timeStr}`;
  }

  // Yearly
  if (timeStr && /^\d+$/.test(dom) && /^\d+$/.test(mon) && dow === "*") {
    const mName = monthNames[parseInt(mon)] ?? mon;
    return `${mName} ${dom} at ${timeStr}`;
  }

  return expr; // fallback: show raw expression
}

function fmtTime(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

/* ── Shared styles ─────────────────────────────────────── */

const cardStyle: React.CSSProperties = {
  background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
  border: "1px solid rgba(0, 240, 255, 0.1)",
  borderRadius: 12,
  overflow: "hidden",
  position: "relative",
};

const inputStyle: React.CSSProperties = {
  background: "rgba(0, 240, 255, 0.04)",
  border: "1px solid rgba(0, 240, 255, 0.12)",
  borderRadius: 6,
  padding: "8px 12px",
  color: "#e2e8f0",
  fontSize: 13,
  fontFamily: "monospace",
  outline: "none",
  width: "100%",
};

const btnPrimary: React.CSSProperties = {
  background: "linear-gradient(135deg, rgba(0, 240, 255, 0.15) 0%, rgba(6, 214, 160, 0.15) 100%)",
  border: "1px solid rgba(0, 240, 255, 0.3)",
  borderRadius: 6,
  padding: "8px 16px",
  color: "#00f0ff",
  fontSize: 12,
  fontWeight: 700,
  letterSpacing: "0.05em",
  cursor: "pointer",
};

const btnDanger: React.CSSProperties = {
  background: "rgba(239, 68, 68, 0.1)",
  border: "1px solid rgba(239, 68, 68, 0.3)",
  borderRadius: 6,
  padding: "6px 12px",
  color: "#ef4444",
  fontSize: 11,
  fontWeight: 700,
  cursor: "pointer",
};

const btnGhost: React.CSSProperties = {
  background: "transparent",
  border: "1px solid rgba(0, 240, 255, 0.15)",
  borderRadius: 6,
  padding: "6px 12px",
  color: "#94a3b8",
  fontSize: 11,
  fontWeight: 700,
  cursor: "pointer",
};

/* ── Component ─────────────────────────────────────────── */

const EMPTY_FORM: ScheduleForm = {
  pipeline_id: "",
  cron_expression: "0 8 * * *",
  timezone: "UTC",
  enabled: true,
};

export default function ScheduleManager({ refreshKey }: { refreshKey?: number }) {
  const [schedules, setSchedules] = useState<Schedule[]>([]);
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const [showForm, setShowForm] = useState(false);
  const [editingId, setEditingId] = useState<number | null>(null);
  const [form, setForm] = useState<ScheduleForm>(EMPTY_FORM);
  const [saving, setSaving] = useState(false);

  /* ── Fetch ───────────────────────────────────────────── */

  const fetchData = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [sRes, pRes] = await Promise.all([
        fetch("/api/stille-post/schedules"),
        fetch("/api/stille-post/pipelines"),
      ]);
      if (!sRes.ok) throw new Error(`Schedules: ${sRes.status}`);
      if (!pRes.ok) throw new Error(`Pipelines: ${pRes.status}`);
      setSchedules(await sRes.json());
      setPipelines(await pRes.json());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Failed to load data");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchData();
  }, [fetchData, refreshKey]);

  /* ── CRUD helpers ────────────────────────────────────── */

  const openCreate = () => {
    setEditingId(null);
    setForm(EMPTY_FORM);
    setShowForm(true);
  };

  const openEdit = (s: Schedule) => {
    setEditingId(s.id);
    setForm({
      pipeline_id: s.pipeline_id,
      cron_expression: s.cron_expression,
      timezone: s.timezone,
      enabled: s.enabled,
    });
    setShowForm(true);
  };

  const cancelForm = () => {
    setShowForm(false);
    setEditingId(null);
  };

  const handleSave = async () => {
    if (form.pipeline_id === "") return;
    setSaving(true);
    try {
      const url = editingId
        ? `/api/stille-post/schedules/${editingId}`
        : "/api/stille-post/schedules";
      const res = await fetch(url, {
        method: editingId ? "PUT" : "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          pipeline_id: Number(form.pipeline_id),
          cron_expression: form.cron_expression,
          timezone: form.timezone,
          enabled: form.enabled,
        }),
      });
      if (!res.ok) throw new Error(`Save failed: ${res.status}`);
      cancelForm();
      fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const handleDelete = async (id: number) => {
    try {
      const res = await fetch(`/api/stille-post/schedules/${id}`, { method: "DELETE" });
      if (!res.ok) throw new Error(`Delete failed: ${res.status}`);
      fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Delete failed");
    }
  };

  const toggleEnabled = async (s: Schedule) => {
    try {
      const res = await fetch(`/api/stille-post/schedules/${s.id}`, {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ ...s, enabled: !s.enabled }),
      });
      if (!res.ok) throw new Error(`Toggle failed: ${res.status}`);
      fetchData();
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Toggle failed");
    }
  };

  /* ── Pipeline name lookup ────────────────────────────── */

  const pipelineName = (id: number) =>
    pipelines.find((p) => p.id === id)?.name ?? `Pipeline #${id}`;

  /* ── Render ──────────────────────────────────────────── */

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div
          className="w-5 h-5 rounded-full animate-spin"
          style={{ border: "2px solid rgba(0,240,255,0.15)", borderTopColor: "#00f0ff" }}
        />
        <span className="ml-3 text-sm text-slate-500 font-mono">Loading schedules…</span>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {/* Header row */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-sm font-bold tracking-wider" style={{ color: "#00f0ff" }}>
            Schedules
          </h2>
          <p className="text-xs text-slate-500 font-mono mt-0.5">
            {schedules.length} schedule{schedules.length !== 1 ? "s" : ""} configured
          </p>
        </div>
        <button style={btnPrimary} onClick={openCreate}>
          + New Schedule
        </button>
      </div>

      {/* Error banner */}
      {error && (
        <div
          className="rounded-lg px-4 py-2 text-xs font-mono"
          style={{ background: "rgba(239,68,68,0.1)", border: "1px solid rgba(239,68,68,0.3)", color: "#ef4444" }}
        >
          {error}
          <button className="ml-3 underline" onClick={() => setError(null)}>dismiss</button>
        </div>
      )}

      {/* Create/Edit form */}
      {showForm && (
        <div style={cardStyle} className="p-5">
          <div
            className="absolute top-0 left-0 w-full h-[1px]"
            style={{ background: "linear-gradient(90deg, transparent, rgba(6,214,160,0.4), transparent)" }}
          />
          <h3 className="text-xs font-bold uppercase tracking-widest mb-4" style={{ color: "#06d6a0" }}>
            {editingId ? "Edit Schedule" : "New Schedule"}
          </h3>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
            {/* Pipeline selector */}
            <div>
              <label className="block text-[10px] font-bold uppercase tracking-wider text-slate-500 mb-1">
                Pipeline
              </label>
              <select
                style={{ ...inputStyle, cursor: "pointer" }}
                value={form.pipeline_id}
                onChange={(e) => setForm({ ...form, pipeline_id: e.target.value === "" ? "" : Number(e.target.value) })}
              >
                <option value="">Select a pipeline…</option>
                {pipelines.map((p) => (
                  <option key={p.id} value={p.id}>{p.name}</option>
                ))}
              </select>
            </div>

            {/* Timezone */}
            <div>
              <label className="block text-[10px] font-bold uppercase tracking-wider text-slate-500 mb-1">
                Timezone
              </label>
              <select
                style={{ ...inputStyle, cursor: "pointer" }}
                value={form.timezone}
                onChange={(e) => setForm({ ...form, timezone: e.target.value })}
              >
                {COMMON_TIMEZONES.map((tz) => (
                  <option key={tz} value={tz}>{tz}</option>
                ))}
              </select>
            </div>

            {/* Cron expression */}
            <div className="md:col-span-2">
              <label className="block text-[10px] font-bold uppercase tracking-wider text-slate-500 mb-1">
                Cron Expression
              </label>
              <input
                style={inputStyle}
                value={form.cron_expression}
                onChange={(e) => setForm({ ...form, cron_expression: e.target.value })}
                placeholder="* * * * *"
              />
              <div className="mt-1.5 flex items-center gap-3 flex-wrap">
                <span
                  className="text-xs font-mono px-2 py-0.5 rounded"
                  style={{ background: "rgba(6,214,160,0.1)", color: "#06d6a0" }}
                >
                  {describeCron(form.cron_expression)}
                </span>
                <span className="text-[10px] text-slate-600">Presets:</span>
                {CRON_PRESETS.map((p) => (
                  <button
                    key={p.cron}
                    className="text-[10px] font-mono px-1.5 py-0.5 rounded transition-colors"
                    style={{
                      background: form.cron_expression === p.cron ? "rgba(0,240,255,0.12)" : "transparent",
                      color: form.cron_expression === p.cron ? "#00f0ff" : "#475569",
                      border: "1px solid rgba(0,240,255,0.08)",
                      cursor: "pointer",
                    }}
                    onClick={() => setForm({ ...form, cron_expression: p.cron })}
                  >
                    {p.label}
                  </button>
                ))}
              </div>
              <div className="mt-1 text-[10px] text-slate-600 font-mono">
                Format: minute hour day-of-month month day-of-week
              </div>
            </div>

            {/* Enabled toggle */}
            <div className="md:col-span-2 flex items-center gap-3">
              <button
                onClick={() => setForm({ ...form, enabled: !form.enabled })}
                className="relative w-10 h-5 rounded-full transition-colors"
                style={{
                  background: form.enabled ? "rgba(6, 214, 160, 0.3)" : "rgba(71, 85, 105, 0.3)",
                  border: `1px solid ${form.enabled ? "rgba(6, 214, 160, 0.5)" : "rgba(71, 85, 105, 0.5)"}`,
                  cursor: "pointer",
                }}
              >
                <div
                  className="absolute top-0.5 w-3.5 h-3.5 rounded-full transition-all"
                  style={{
                    left: form.enabled ? 20 : 3,
                    background: form.enabled ? "#06d6a0" : "#475569",
                  }}
                />
              </button>
              <span className="text-xs" style={{ color: form.enabled ? "#06d6a0" : "#475569" }}>
                {form.enabled ? "Enabled" : "Disabled"}
              </span>
            </div>
          </div>

          {/* Actions */}
          <div className="flex items-center gap-3 mt-5 pt-4" style={{ borderTop: "1px solid rgba(0,240,255,0.06)" }}>
            <button
              style={{ ...btnPrimary, opacity: saving || form.pipeline_id === "" ? 0.5 : 1 }}
              disabled={saving || form.pipeline_id === ""}
              onClick={handleSave}
            >
              {saving ? "Saving…" : editingId ? "Update Schedule" : "Create Schedule"}
            </button>
            <button style={btnGhost} onClick={cancelForm}>
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Schedule list */}
      {schedules.length === 0 ? (
        <div style={cardStyle} className="p-8 text-center">
          <div
            className="absolute top-0 left-0 w-full h-[1px]"
            style={{ background: "linear-gradient(90deg, transparent, rgba(6,214,160,0.4), transparent)" }}
          />
          <div className="text-2xl mb-2" style={{ color: "#475569" }}>
            &#128197;
          </div>
          <p className="text-sm text-slate-500 font-mono">No schedules configured yet</p>
          <button className="mt-3" style={btnPrimary} onClick={openCreate}>
            Create your first schedule
          </button>
        </div>
      ) : (
        <div style={cardStyle}>
          <div
            className="absolute top-0 left-0 w-full h-[1px]"
            style={{ background: "linear-gradient(90deg, transparent, rgba(6,214,160,0.4), transparent)" }}
          />
          <table className="w-full text-left">
            <thead>
              <tr style={{ borderBottom: "1px solid rgba(0,240,255,0.06)" }}>
                {["Pipeline", "Schedule", "Timezone", "Enabled", "Last Run", "Next Run", ""].map(
                  (h) => (
                    <th
                      key={h}
                      className="px-4 py-3 text-[10px] font-bold uppercase tracking-widest"
                      style={{ color: "#475569" }}
                    >
                      {h}
                    </th>
                  ),
                )}
              </tr>
            </thead>
            <tbody>
              {schedules.map((s) => (
                <tr
                  key={s.id}
                  className="group transition-colors"
                  style={{ borderBottom: "1px solid rgba(0,240,255,0.04)" }}
                  onMouseEnter={(e) => (e.currentTarget.style.background = "rgba(0,240,255,0.02)")}
                  onMouseLeave={(e) => (e.currentTarget.style.background = "transparent")}
                >
                  {/* Pipeline */}
                  <td className="px-4 py-3">
                    <span className="text-xs font-bold" style={{ color: "#e2e8f0" }}>
                      {pipelineName(s.pipeline_id)}
                    </span>
                  </td>

                  {/* Cron */}
                  <td className="px-4 py-3">
                    <div className="text-xs font-mono" style={{ color: "#00f0ff" }}>
                      {s.cron_expression}
                    </div>
                    <div className="text-[10px] text-slate-500">{describeCron(s.cron_expression)}</div>
                  </td>

                  {/* Timezone */}
                  <td className="px-4 py-3 text-xs text-slate-400 font-mono">{s.timezone}</td>

                  {/* Enabled toggle */}
                  <td className="px-4 py-3">
                    <button
                      onClick={() => toggleEnabled(s)}
                      className="relative w-9 h-[18px] rounded-full transition-colors"
                      style={{
                        background: s.enabled ? "rgba(6,214,160,0.3)" : "rgba(71,85,105,0.3)",
                        border: `1px solid ${s.enabled ? "rgba(6,214,160,0.5)" : "rgba(71,85,105,0.5)"}`,
                        cursor: "pointer",
                      }}
                    >
                      <div
                        className="absolute top-[2px] w-3 h-3 rounded-full transition-all"
                        style={{
                          left: s.enabled ? 18 : 3,
                          background: s.enabled ? "#06d6a0" : "#475569",
                        }}
                      />
                    </button>
                  </td>

                  {/* Last run */}
                  <td className="px-4 py-3 text-xs text-slate-500 font-mono">{fmtTime(s.last_run_at)}</td>

                  {/* Next run */}
                  <td className="px-4 py-3 text-xs font-mono" style={{ color: s.next_run_at ? "#06d6a0" : "#475569" }}>
                    {fmtTime(s.next_run_at)}
                  </td>

                  {/* Actions */}
                  <td className="px-4 py-3">
                    <div className="flex items-center gap-2 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button style={btnGhost} onClick={() => openEdit(s)}>
                        Edit
                      </button>
                      <button style={btnDanger} onClick={() => handleDelete(s.id)}>
                        Delete
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}
