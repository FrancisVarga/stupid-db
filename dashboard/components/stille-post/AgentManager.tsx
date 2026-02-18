"use client";

import { useState, useEffect, useCallback } from "react";

// ── Types ─────────────────────────────────────────────────────────

interface Agent {
  id: string;
  name: string;
  description: string | null;
  system_prompt: string;
  model: string;
  skills_config: unknown[] | null;
  mcp_servers_config: unknown[] | null;
  tools_config: unknown[] | null;
  template_id: string | null;
  created_at: string;
  updated_at: string;
}

type AgentFormData = Omit<Agent, "id" | "created_at" | "updated_at">;

const MODELS = [
  { value: "claude-sonnet-4-6", label: "Sonnet 4.6" },
  { value: "claude-opus-4-6", label: "Opus 4.6" },
  { value: "claude-haiku-4-5", label: "Haiku 4.5" },
];

const TEMPLATES = [
  { id: "", name: "— No template —", icon: "" },
  { id: "security-analyst", name: "Security Analyst", icon: "🛡️" },
  { id: "trend-detective", name: "Trend Detective", icon: "📈" },
  { id: "performance-monitor", name: "Performance Monitor", icon: "⚡" },
  { id: "executive-summarizer", name: "Executive Summarizer", icon: "📋" },
  { id: "data-quality-auditor", name: "Data Quality Auditor", icon: "🔍" },
];

const EMPTY_FORM: AgentFormData = {
  name: "",
  description: null,
  system_prompt: "",
  model: "claude-sonnet-4-6",
  skills_config: null,
  mcp_servers_config: null,
  tools_config: null,
  template_id: null,
};

// ── Styling constants ─────────────────────────────────────────────

const CYAN = "#00f0ff";
const GREEN = "#06d6a0";

const cardStyle: React.CSSProperties = {
  background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
  border: "1px solid rgba(0, 240, 255, 0.1)",
  borderRadius: 12,
};

const inputStyle: React.CSSProperties = {
  background: "rgba(15, 23, 42, 0.8)",
  border: "1px solid rgba(0, 240, 255, 0.15)",
  borderRadius: 8,
  color: "#e2e8f0",
  padding: "8px 12px",
  fontSize: 13,
  fontFamily: "monospace",
  width: "100%",
  outline: "none",
};

const btnPrimary: React.CSSProperties = {
  background: `linear-gradient(135deg, ${CYAN}22, ${GREEN}22)`,
  border: `1px solid ${CYAN}44`,
  borderRadius: 8,
  color: CYAN,
  padding: "8px 20px",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};

const btnDanger: React.CSSProperties = {
  background: "rgba(239, 68, 68, 0.1)",
  border: "1px solid rgba(239, 68, 68, 0.3)",
  borderRadius: 8,
  color: "#ef4444",
  padding: "6px 14px",
  fontSize: 12,
  fontWeight: 600,
  cursor: "pointer",
};

const btnGhost: React.CSSProperties = {
  background: "transparent",
  border: "1px solid rgba(100, 116, 139, 0.3)",
  borderRadius: 8,
  color: "#94a3b8",
  padding: "8px 20px",
  fontSize: 13,
  fontWeight: 600,
  cursor: "pointer",
};

// ── Helpers ───────────────────────────────────────────────────────

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString("en-US", {
    month: "short",
    day: "numeric",
    year: "numeric",
  });
}

function templateLabel(id: string | null): string {
  if (!id) return "—";
  const t = TEMPLATES.find((t) => t.id === id);
  return t ? `${t.icon} ${t.name}` : id;
}

function safeJsonParse(s: string): unknown[] | null {
  if (!s.trim()) return null;
  try {
    const parsed = JSON.parse(s);
    return Array.isArray(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

// ── Component ─────────────────────────────────────────────────────

interface AgentManagerProps {
  refreshKey: number;
}

export default function AgentManager({ refreshKey }: AgentManagerProps) {
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<"create" | "edit">("create");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<AgentFormData>({ ...EMPTY_FORM });
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState<Agent | null>(null);
  const [deleting, setDeleting] = useState(false);

  // JSON editor strings (kept as text for editing)
  const [skillsText, setSkillsText] = useState("[]");
  const [mcpText, setMcpText] = useState("[]");
  const [toolsText, setToolsText] = useState("[]");

  // ── Fetch ─────────────────────────────────────────────────────

  const fetchAgents = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/stille-post/agents");
      if (!res.ok) throw new Error(`Failed to fetch agents (${res.status})`);
      const data = await res.json();
      setAgents(Array.isArray(data) ? data : data.agents ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    fetchAgents();
  }, [fetchAgents, refreshKey]);

  // ── Modal open helpers ────────────────────────────────────────

  function openCreate() {
    setForm({ ...EMPTY_FORM });
    setSkillsText("[]");
    setMcpText("[]");
    setToolsText("[]");
    setModalMode("create");
    setEditingId(null);
    setFormError(null);
    setModalOpen(true);
  }

  function openEdit(agent: Agent) {
    setForm({
      name: agent.name,
      description: agent.description,
      system_prompt: agent.system_prompt,
      model: agent.model,
      skills_config: agent.skills_config,
      mcp_servers_config: agent.mcp_servers_config,
      tools_config: agent.tools_config,
      template_id: agent.template_id,
    });
    setSkillsText(JSON.stringify(agent.skills_config ?? [], null, 2));
    setMcpText(JSON.stringify(agent.mcp_servers_config ?? [], null, 2));
    setToolsText(JSON.stringify(agent.tools_config ?? [], null, 2));
    setModalMode("edit");
    setEditingId(agent.id);
    setFormError(null);
    setModalOpen(true);
  }

  // ── Submit ────────────────────────────────────────────────────

  async function handleSubmit() {
    if (!form.name.trim()) {
      setFormError("Name is required");
      return;
    }
    if (!form.system_prompt.trim()) {
      setFormError("System prompt is required");
      return;
    }

    setSubmitting(true);
    setFormError(null);

    const body = {
      ...form,
      description: form.description?.trim() || null,
      skills_config: safeJsonParse(skillsText),
      mcp_servers_config: safeJsonParse(mcpText),
      tools_config: safeJsonParse(toolsText),
      template_id: form.template_id || null,
    };

    try {
      const url =
        modalMode === "create"
          ? "/api/stille-post/agents"
          : `/api/stille-post/agents/${editingId}`;
      const method = modalMode === "create" ? "POST" : "PUT";

      const res = await fetch(url, {
        method,
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });

      if (!res.ok) {
        const text = await res.text();
        throw new Error(text || `Request failed (${res.status})`);
      }

      setModalOpen(false);
      fetchAgents();
    } catch (e) {
      setFormError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setSubmitting(false);
    }
  }

  // ── Delete ────────────────────────────────────────────────────

  async function handleDelete() {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      const res = await fetch(`/api/stille-post/agents/${deleteTarget.id}`, {
        method: "DELETE",
      });
      if (!res.ok) throw new Error(`Delete failed (${res.status})`);
      setDeleteTarget(null);
      fetchAgents();
    } catch (e) {
      setFormError(e instanceof Error ? e.message : "Delete failed");
    } finally {
      setDeleting(false);
    }
  }

  // ── Render ────────────────────────────────────────────────────

  if (loading && agents.length === 0) {
    return (
      <div className="flex items-center justify-center py-16">
        <div
          className="text-xs font-mono tracking-wider"
          style={{ color: CYAN }}
        >
          Loading agents...
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-xl p-6" style={cardStyle}>
        <div className="text-red-400 text-sm font-mono">{error}</div>
        <button
          onClick={fetchAgents}
          style={btnGhost}
          className="mt-3"
        >
          Retry
        </button>
      </div>
    );
  }

  return (
    <div>
      {/* Header row */}
      <div className="flex items-center justify-between mb-4">
        <div className="text-xs font-mono text-slate-500">
          {agents.length} agent{agents.length !== 1 ? "s" : ""}
        </div>
        <button onClick={openCreate} style={btnPrimary}>
          + New Agent
        </button>
      </div>

      {/* Agent table */}
      {agents.length === 0 ? (
        <div className="rounded-xl p-12 text-center" style={cardStyle}>
          <div
            className="text-[10px] font-bold uppercase tracking-[0.15em] mb-2"
            style={{ color: GREEN }}
          >
            No agents yet
          </div>
          <p className="text-sm text-slate-500 font-mono mb-4">
            Create your first agent to get started
          </p>
          <button onClick={openCreate} style={btnPrimary}>
            + Create Agent
          </button>
        </div>
      ) : (
        <div className="rounded-xl overflow-hidden" style={cardStyle}>
          <table className="w-full text-sm">
            <thead>
              <tr
                style={{
                  borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
                  background: "rgba(0, 240, 255, 0.02)",
                }}
              >
                <th
                  className="text-left px-4 py-3 text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: CYAN }}
                >
                  Name
                </th>
                <th
                  className="text-left px-4 py-3 text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: CYAN }}
                >
                  Model
                </th>
                <th
                  className="text-left px-4 py-3 text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: CYAN }}
                >
                  Template
                </th>
                <th
                  className="text-left px-4 py-3 text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: CYAN }}
                >
                  Created
                </th>
                <th className="px-4 py-3 w-24" />
              </tr>
            </thead>
            <tbody>
              {agents.map((agent) => (
                <tr
                  key={agent.id}
                  className="group cursor-pointer"
                  style={{
                    borderBottom: "1px solid rgba(0, 240, 255, 0.04)",
                  }}
                  onClick={() => openEdit(agent)}
                  onMouseOver={(e) => {
                    (e.currentTarget as HTMLElement).style.background =
                      "rgba(0, 240, 255, 0.03)";
                  }}
                  onMouseOut={(e) => {
                    (e.currentTarget as HTMLElement).style.background = "";
                  }}
                >
                  <td className="px-4 py-3">
                    <div className="font-semibold text-slate-200">
                      {agent.name}
                    </div>
                    {agent.description && (
                      <div className="text-xs text-slate-500 mt-0.5 truncate max-w-sm">
                        {agent.description}
                      </div>
                    )}
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className="text-xs font-mono px-2 py-1 rounded"
                      style={{
                        background: "rgba(0, 240, 255, 0.08)",
                        color: CYAN,
                      }}
                    >
                      {agent.model}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs text-slate-400">
                    {templateLabel(agent.template_id)}
                  </td>
                  <td className="px-4 py-3 text-xs text-slate-500 font-mono">
                    {formatDate(agent.created_at)}
                  </td>
                  <td className="px-4 py-3">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteTarget(agent);
                      }}
                      style={{
                        ...btnDanger,
                        opacity: 0.6,
                        transition: "opacity 150ms",
                      }}
                      onMouseOver={(e) => {
                        (e.currentTarget as HTMLElement).style.opacity = "1";
                      }}
                      onMouseOut={(e) => {
                        (e.currentTarget as HTMLElement).style.opacity = "0.6";
                      }}
                    >
                      Delete
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {/* ── Create/Edit Modal ──────────────────────────────────── */}
      {modalOpen && (
        <div
          className="fixed inset-0 z-50 flex items-start justify-center pt-12"
          style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
          onClick={() => setModalOpen(false)}
        >
          <div
            className="w-full max-w-2xl max-h-[85vh] overflow-y-auto rounded-xl p-6 relative"
            style={{
              ...cardStyle,
              border: `1px solid ${CYAN}22`,
              boxShadow: `0 0 40px rgba(0, 240, 255, 0.05)`,
            }}
            onClick={(e) => e.stopPropagation()}
          >
            {/* Top accent */}
            <div
              className="absolute top-0 left-0 w-full h-[1px]"
              style={{
                background: `linear-gradient(90deg, transparent, ${GREEN}66, transparent)`,
              }}
            />

            <h2
              className="text-sm font-bold uppercase tracking-wider mb-5"
              style={{ color: CYAN }}
            >
              {modalMode === "create" ? "Create Agent" : "Edit Agent"}
            </h2>

            {formError && (
              <div
                className="rounded-lg px-4 py-2 mb-4 text-xs font-mono"
                style={{
                  background: "rgba(239, 68, 68, 0.1)",
                  border: "1px solid rgba(239, 68, 68, 0.25)",
                  color: "#ef4444",
                }}
              >
                {formError}
              </div>
            )}

            {/* Name */}
            <label className="block mb-4">
              <span
                className="text-[10px] font-bold uppercase tracking-wider block mb-1.5"
                style={{ color: "#94a3b8" }}
              >
                Name *
              </span>
              <input
                type="text"
                value={form.name}
                onChange={(e) => setForm({ ...form, name: e.target.value })}
                placeholder="My Security Agent"
                style={inputStyle}
              />
            </label>

            {/* Description */}
            <label className="block mb-4">
              <span
                className="text-[10px] font-bold uppercase tracking-wider block mb-1.5"
                style={{ color: "#94a3b8" }}
              >
                Description
              </span>
              <input
                type="text"
                value={form.description ?? ""}
                onChange={(e) =>
                  setForm({ ...form, description: e.target.value || null })
                }
                placeholder="Brief description of what this agent does"
                style={inputStyle}
              />
            </label>

            {/* Model + Template row */}
            <div className="grid grid-cols-2 gap-4 mb-4">
              <label className="block">
                <span
                  className="text-[10px] font-bold uppercase tracking-wider block mb-1.5"
                  style={{ color: "#94a3b8" }}
                >
                  Model
                </span>
                <select
                  value={form.model}
                  onChange={(e) => setForm({ ...form, model: e.target.value })}
                  style={{
                    ...inputStyle,
                    appearance: "auto" as React.CSSProperties["appearance"],
                  }}
                >
                  {MODELS.map((m) => (
                    <option key={m.value} value={m.value}>
                      {m.label}
                    </option>
                  ))}
                </select>
              </label>

              <label className="block">
                <span
                  className="text-[10px] font-bold uppercase tracking-wider block mb-1.5"
                  style={{ color: "#94a3b8" }}
                >
                  Template
                </span>
                <select
                  value={form.template_id ?? ""}
                  onChange={(e) =>
                    setForm({ ...form, template_id: e.target.value || null })
                  }
                  style={{
                    ...inputStyle,
                    appearance: "auto" as React.CSSProperties["appearance"],
                  }}
                >
                  {TEMPLATES.map((t) => (
                    <option key={t.id} value={t.id}>
                      {t.icon ? `${t.icon} ${t.name}` : t.name}
                    </option>
                  ))}
                </select>
              </label>
            </div>

            {/* System prompt */}
            <label className="block mb-4">
              <span
                className="text-[10px] font-bold uppercase tracking-wider block mb-1.5"
                style={{ color: "#94a3b8" }}
              >
                System Prompt *
              </span>
              <textarea
                value={form.system_prompt}
                onChange={(e) =>
                  setForm({ ...form, system_prompt: e.target.value })
                }
                placeholder="You are a..."
                rows={8}
                style={{
                  ...inputStyle,
                  resize: "vertical",
                  lineHeight: 1.5,
                }}
              />
            </label>

            {/* JSON config editors */}
            <div className="space-y-4 mb-6">
              <JsonConfigField
                label="Skills Config"
                value={skillsText}
                onChange={setSkillsText}
              />
              <JsonConfigField
                label="MCP Servers Config"
                value={mcpText}
                onChange={setMcpText}
              />
              <JsonConfigField
                label="Tools Config"
                value={toolsText}
                onChange={setToolsText}
              />
            </div>

            {/* Actions */}
            <div className="flex items-center justify-end gap-3">
              <button
                onClick={() => setModalOpen(false)}
                style={btnGhost}
                disabled={submitting}
              >
                Cancel
              </button>
              <button
                onClick={handleSubmit}
                style={{
                  ...btnPrimary,
                  opacity: submitting ? 0.5 : 1,
                }}
                disabled={submitting}
              >
                {submitting
                  ? "Saving..."
                  : modalMode === "create"
                    ? "Create Agent"
                    : "Save Changes"}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* ── Delete Confirmation Modal ──────────────────────────── */}
      {deleteTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center"
          style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
          onClick={() => !deleting && setDeleteTarget(null)}
        >
          <div
            className="w-full max-w-md rounded-xl p-6"
            style={{
              ...cardStyle,
              border: "1px solid rgba(239, 68, 68, 0.2)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <h3
              className="text-sm font-bold uppercase tracking-wider mb-3"
              style={{ color: "#ef4444" }}
            >
              Delete Agent
            </h3>
            <p className="text-sm text-slate-400 mb-5">
              Are you sure you want to delete{" "}
              <span className="font-semibold text-slate-200">
                {deleteTarget.name}
              </span>
              ? This action cannot be undone.
            </p>
            <div className="flex justify-end gap-3">
              <button
                onClick={() => setDeleteTarget(null)}
                style={btnGhost}
                disabled={deleting}
              >
                Cancel
              </button>
              <button
                onClick={handleDelete}
                style={{
                  ...btnDanger,
                  opacity: deleting ? 0.5 : 1,
                }}
                disabled={deleting}
              >
                {deleting ? "Deleting..." : "Delete"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ── JSON Config Sub-component ───────────────────────────────────

function JsonConfigField({
  label,
  value,
  onChange,
}: {
  label: string;
  value: string;
  onChange: (v: string) => void;
}) {
  const [expanded, setExpanded] = useState(false);

  const isValid = (() => {
    try {
      JSON.parse(value);
      return true;
    } catch {
      return false;
    }
  })();

  return (
    <div>
      <button
        onClick={() => setExpanded(!expanded)}
        className="flex items-center gap-2 w-full text-left"
        type="button"
      >
        <span
          className="text-[10px] font-bold uppercase tracking-wider"
          style={{ color: "#94a3b8" }}
        >
          {label}
        </span>
        <span className="text-[10px] text-slate-600">
          {expanded ? "▼" : "▶"}
        </span>
        {!isValid && (
          <span className="text-[10px] text-red-400 font-mono">
            invalid JSON
          </span>
        )}
      </button>
      {expanded && (
        <textarea
          value={value}
          onChange={(e) => onChange(e.target.value)}
          rows={4}
          style={{
            ...inputStyle,
            marginTop: 6,
            resize: "vertical",
            borderColor: isValid
              ? "rgba(0, 240, 255, 0.15)"
              : "rgba(239, 68, 68, 0.4)",
          }}
        />
      )}
    </div>
  );
}
