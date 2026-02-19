"use client";

import { useState, useEffect, useCallback, useRef } from "react";
import * as d3 from "d3";

// ── Types ─────────────────────────────────────────────────────────

interface Pipeline {
  id: string;
  name: string;
  description: string | null;
  step_count: number;
  created_at: string;
  updated_at: string;
}

interface PipelineStep {
  id?: string;
  step_order: number;
  agent_id: string | null;
  parallel_group: number | null;
  input_mapping: Record<string, unknown> | null;
}

interface PipelineDetail extends Pipeline {
  steps: PipelineStep[];
}

interface Agent {
  id: string;
  name: string;
  description: string | null;
}

interface PipelineFormData {
  name: string;
  description: string | null;
  steps: PipelineStep[];
}

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

const btnSmall: React.CSSProperties = {
  background: `linear-gradient(135deg, ${CYAN}11, ${GREEN}11)`,
  border: `1px solid ${CYAN}33`,
  borderRadius: 6,
  color: CYAN,
  padding: "4px 12px",
  fontSize: 11,
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

function agentName(agents: Agent[], id: string | null): string {
  if (!id) return "(unassigned)";
  const a = agents.find((a) => a.id === id);
  return a ? a.name : id.slice(0, 8) + "...";
}

const EMPTY_FORM: PipelineFormData = {
  name: "",
  description: null,
  steps: [],
};

// ── D3 Pipeline Diagram ───────────────────────────────────────────

function PipelineDiagram({
  steps,
  agents,
}: {
  steps: PipelineStep[];
  agents: Agent[];
}) {
  const svgRef = useRef<SVGSVGElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!svgRef.current || !containerRef.current || steps.length === 0) return;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const containerWidth = containerRef.current.clientWidth;
    const nodeWidth = 180;
    const nodeHeight = 52;
    const verticalGap = 28;
    const parallelGap = 16;

    // Group steps by order, then by parallel_group
    const ordered = [...steps].sort((a, b) => a.step_order - b.step_order);
    const groups: { order: number; items: PipelineStep[] }[] = [];
    let currentOrder = -1;
    for (const step of ordered) {
      if (step.step_order !== currentOrder) {
        groups.push({ order: step.step_order, items: [step] });
        currentOrder = step.step_order;
      } else {
        groups[groups.length - 1].items.push(step);
      }
    }

    // Calculate positions
    interface NodePos {
      step: PipelineStep;
      x: number;
      y: number;
      groupIdx: number;
    }
    const nodes: NodePos[] = [];
    let yOffset = 20;

    groups.forEach((group, gi) => {
      const count = group.items.length;
      const totalWidth = count * nodeWidth + (count - 1) * parallelGap;
      const startX = (containerWidth - totalWidth) / 2;

      group.items.forEach((step, i) => {
        nodes.push({
          step,
          x: startX + i * (nodeWidth + parallelGap),
          y: yOffset,
          groupIdx: gi,
        });
      });

      yOffset += nodeHeight + verticalGap;
    });

    const totalHeight = yOffset + 10;
    svg.attr("width", containerWidth).attr("height", totalHeight);

    // Draw connection lines between groups
    const defs = svg.append("defs");
    const gradient = defs
      .append("linearGradient")
      .attr("id", "pipe-line-grad")
      .attr("x1", "0%")
      .attr("y1", "0%")
      .attr("x2", "0%")
      .attr("y2", "100%");
    gradient.append("stop").attr("offset", "0%").attr("stop-color", CYAN).attr("stop-opacity", 0.6);
    gradient.append("stop").attr("offset", "100%").attr("stop-color", GREEN).attr("stop-opacity", 0.6);

    // Arrow marker
    defs
      .append("marker")
      .attr("id", "pipe-arrow")
      .attr("viewBox", "0 0 10 10")
      .attr("refX", 10)
      .attr("refY", 5)
      .attr("markerWidth", 6)
      .attr("markerHeight", 6)
      .attr("orient", "auto")
      .append("path")
      .attr("d", "M 0 0 L 10 5 L 0 10 Z")
      .attr("fill", CYAN)
      .attr("opacity", 0.5);

    // Draw edges between consecutive groups
    for (let gi = 0; gi < groups.length - 1; gi++) {
      const sourceNodes = nodes.filter((n) => n.groupIdx === gi);
      const targetNodes = nodes.filter((n) => n.groupIdx === gi + 1);

      sourceNodes.forEach((src) => {
        targetNodes.forEach((tgt) => {
          const srcCx = src.x + nodeWidth / 2;
          const srcCy = src.y + nodeHeight;
          const tgtCx = tgt.x + nodeWidth / 2;
          const tgtCy = tgt.y;

          svg
            .append("path")
            .attr(
              "d",
              `M ${srcCx} ${srcCy} C ${srcCx} ${srcCy + 14}, ${tgtCx} ${tgtCy - 14}, ${tgtCx} ${tgtCy}`
            )
            .attr("fill", "none")
            .attr("stroke", "url(#pipe-line-grad)")
            .attr("stroke-width", 1.5)
            .attr("marker-end", "url(#pipe-arrow)");
        });
      });
    }

    // Draw step nodes
    const nodeGroups = svg
      .selectAll("g.step-node")
      .data(nodes)
      .enter()
      .append("g")
      .attr("class", "step-node")
      .attr("transform", (d) => `translate(${d.x}, ${d.y})`);

    // Node background
    nodeGroups
      .append("rect")
      .attr("width", nodeWidth)
      .attr("height", nodeHeight)
      .attr("rx", 8)
      .attr("fill", "#0c1018")
      .attr("stroke", (d) =>
        d.step.parallel_group !== null ? GREEN : CYAN
      )
      .attr("stroke-opacity", 0.3)
      .attr("stroke-width", 1);

    // Step order badge
    nodeGroups
      .append("circle")
      .attr("cx", 16)
      .attr("cy", nodeHeight / 2)
      .attr("r", 10)
      .attr("fill", (d) =>
        d.step.parallel_group !== null
          ? "rgba(6, 214, 160, 0.15)"
          : "rgba(0, 240, 255, 0.15)"
      );

    nodeGroups
      .append("text")
      .attr("x", 16)
      .attr("y", nodeHeight / 2 + 1)
      .attr("text-anchor", "middle")
      .attr("dominant-baseline", "central")
      .attr("fill", (d) =>
        d.step.parallel_group !== null ? GREEN : CYAN
      )
      .attr("font-size", 10)
      .attr("font-weight", 700)
      .attr("font-family", "monospace")
      .text((d) => String(d.step.step_order + 1));

    // Agent name
    nodeGroups
      .append("text")
      .attr("x", 34)
      .attr("y", nodeHeight / 2 - 6)
      .attr("fill", "#e2e8f0")
      .attr("font-size", 11)
      .attr("font-weight", 600)
      .text((d) => {
        const name = agentName(agents, d.step.agent_id);
        return name.length > 16 ? name.slice(0, 15) + "..." : name;
      });

    // Parallel group label
    nodeGroups
      .append("text")
      .attr("x", 34)
      .attr("y", nodeHeight / 2 + 10)
      .attr("fill", "#64748b")
      .attr("font-size", 9)
      .attr("font-family", "monospace")
      .text((d) =>
        d.step.parallel_group !== null
          ? `parallel #${d.step.parallel_group}`
          : "sequential"
      );
  }, [steps, agents]);

  if (steps.length === 0) {
    return (
      <div
        className="text-center py-8 text-xs font-mono"
        style={{ color: "#64748b" }}
      >
        No steps yet. Add steps to visualize the pipeline.
      </div>
    );
  }

  return (
    <div ref={containerRef} style={{ width: "100%", overflow: "hidden" }}>
      <svg ref={svgRef} />
    </div>
  );
}

// ── Component ─────────────────────────────────────────────────────

interface PipelineBuilderProps {
  refreshKey: number;
}

export default function PipelineBuilder({ refreshKey }: PipelineBuilderProps) {
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [agents, setAgents] = useState<Agent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [modalMode, setModalMode] = useState<"create" | "edit">("create");
  const [editingId, setEditingId] = useState<string | null>(null);
  const [form, setForm] = useState<PipelineFormData>({ ...EMPTY_FORM });
  const [submitting, setSubmitting] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  // Step being configured (index into form.steps)
  const [editingStepIdx, setEditingStepIdx] = useState<number | null>(null);
  const [inputMappingText, setInputMappingText] = useState("{}");

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState<Pipeline | null>(null);
  const [deleting, setDeleting] = useState(false);

  // ── Fetch ─────────────────────────────────────────────────────

  const fetchPipelines = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/stille-post/pipelines");
      if (!res.ok) throw new Error(`Failed to fetch pipelines (${res.status})`);
      const data = await res.json();
      setPipelines(Array.isArray(data) ? data : data.pipelines ?? []);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }, []);

  const fetchAgents = useCallback(async () => {
    try {
      const res = await fetch("/api/stille-post/agents");
      if (!res.ok) return;
      const data = await res.json();
      setAgents(Array.isArray(data) ? data : data.agents ?? []);
    } catch {
      // Agents are supplementary; don't block on failure
    }
  }, []);

  useEffect(() => {
    fetchPipelines();
    fetchAgents();
  }, [fetchPipelines, fetchAgents, refreshKey]);

  // ── Modal open helpers ────────────────────────────────────────

  function openCreate() {
    setForm({ ...EMPTY_FORM, steps: [] });
    setModalMode("create");
    setEditingId(null);
    setEditingStepIdx(null);
    setInputMappingText("{}");
    setFormError(null);
    setModalOpen(true);
  }

  async function openEdit(pipeline: Pipeline) {
    setFormError(null);
    try {
      const res = await fetch(`/api/stille-post/pipelines/${pipeline.id}`);
      if (!res.ok) throw new Error(`Failed to load pipeline (${res.status})`);
      const detail: PipelineDetail = await res.json();
      setForm({
        name: detail.name,
        description: detail.description,
        steps: (detail.steps ?? [])
          .sort((a, b) => a.step_order - b.step_order)
          .map((s) => ({
            step_order: s.step_order,
            agent_id: s.agent_id,
            parallel_group: s.parallel_group,
            input_mapping: s.input_mapping,
          })),
      });
      setModalMode("edit");
      setEditingId(pipeline.id);
      setEditingStepIdx(null);
      setInputMappingText("{}");
      setModalOpen(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load pipeline");
    }
  }

  // ── Step management ───────────────────────────────────────────

  function addStep() {
    if (agents.length === 0) return;
    const newStep: PipelineStep = {
      step_order: form.steps.length,
      agent_id: agents[0].id,
      parallel_group: null,
      input_mapping: null,
    };
    setForm({ ...form, steps: [...form.steps, newStep] });
  }

  function removeStep(idx: number) {
    const newSteps = form.steps
      .filter((_, i) => i !== idx)
      .map((s, i) => ({ ...s, step_order: i }));
    setForm({ ...form, steps: newSteps });
    if (editingStepIdx === idx) setEditingStepIdx(null);
    else if (editingStepIdx !== null && editingStepIdx > idx)
      setEditingStepIdx(editingStepIdx - 1);
  }

  function moveStep(idx: number, direction: "up" | "down") {
    const swapIdx = direction === "up" ? idx - 1 : idx + 1;
    if (swapIdx < 0 || swapIdx >= form.steps.length) return;
    const newSteps = [...form.steps];
    [newSteps[idx], newSteps[swapIdx]] = [newSteps[swapIdx], newSteps[idx]];
    const reordered = newSteps.map((s, i) => ({ ...s, step_order: i }));
    setForm({ ...form, steps: reordered });
    // Track which step is being edited
    if (editingStepIdx === idx) setEditingStepIdx(swapIdx);
    else if (editingStepIdx === swapIdx) setEditingStepIdx(idx);
  }

  function updateStep(idx: number, patch: Partial<PipelineStep>) {
    const newSteps = form.steps.map((s, i) =>
      i === idx ? { ...s, ...patch } : s
    );
    setForm({ ...form, steps: newSteps });
  }

  function openStepConfig(idx: number) {
    setEditingStepIdx(idx);
    const step = form.steps[idx];
    setInputMappingText(
      step.input_mapping ? JSON.stringify(step.input_mapping, null, 2) : "{}"
    );
  }

  function saveStepMapping() {
    if (editingStepIdx === null) return;
    try {
      const parsed = JSON.parse(inputMappingText);
      const mapping =
        typeof parsed === "object" && parsed !== null && !Array.isArray(parsed)
          ? (parsed as Record<string, unknown>)
          : null;
      updateStep(editingStepIdx, { input_mapping: mapping });
      setEditingStepIdx(null);
    } catch {
      // Keep editor open on invalid JSON
    }
  }

  // ── Submit ────────────────────────────────────────────────────

  async function handleSubmit() {
    if (!form.name.trim()) {
      setFormError("Name is required");
      return;
    }

    setSubmitting(true);
    setFormError(null);

    const body = {
      name: form.name.trim(),
      description: form.description?.trim() || null,
      steps: form.steps.map((s) => ({
        step_order: s.step_order,
        agent_id: s.agent_id,
        parallel_group: s.parallel_group,
        input_mapping: s.input_mapping,
      })),
    };

    try {
      const url =
        modalMode === "create"
          ? "/api/stille-post/pipelines"
          : `/api/stille-post/pipelines/${editingId}`;
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
      fetchPipelines();
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
      const res = await fetch(`/api/stille-post/pipelines/${deleteTarget.id}`, {
        method: "DELETE",
      });
      if (!res.ok) throw new Error(`Delete failed (${res.status})`);
      setDeleteTarget(null);
      fetchPipelines();
    } catch (e) {
      setFormError(e instanceof Error ? e.message : "Delete failed");
    } finally {
      setDeleting(false);
    }
  }

  // ── Render ────────────────────────────────────────────────────

  if (loading && pipelines.length === 0) {
    return (
      <div className="flex items-center justify-center py-16">
        <div
          className="text-xs font-mono tracking-wider"
          style={{ color: CYAN }}
        >
          Loading pipelines...
        </div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="rounded-xl p-6" style={cardStyle}>
        <div className="text-red-400 text-sm font-mono">{error}</div>
        <button
          onClick={fetchPipelines}
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
          {pipelines.length} pipeline{pipelines.length !== 1 ? "s" : ""}
        </div>
        <button onClick={openCreate} style={btnPrimary}>
          + New Pipeline
        </button>
      </div>

      {/* Pipeline table */}
      {pipelines.length === 0 ? (
        <div className="rounded-xl p-12 text-center" style={cardStyle}>
          <div
            className="text-[10px] font-bold uppercase tracking-[0.15em] mb-2"
            style={{ color: GREEN }}
          >
            No pipelines yet
          </div>
          <p className="text-sm text-slate-500 font-mono mb-4">
            Create your first pipeline to orchestrate agents
          </p>
          <button onClick={openCreate} style={btnPrimary}>
            + Create Pipeline
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
                  Steps
                </th>
                <th
                  className="text-left px-4 py-3 text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: CYAN }}
                >
                  Description
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
              {pipelines.map((pipeline) => (
                <tr
                  key={pipeline.id}
                  className="group cursor-pointer"
                  style={{
                    borderBottom: "1px solid rgba(0, 240, 255, 0.04)",
                  }}
                  onClick={() => openEdit(pipeline)}
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
                      {pipeline.name}
                    </div>
                  </td>
                  <td className="px-4 py-3">
                    <span
                      className="text-xs font-mono px-2 py-1 rounded"
                      style={{
                        background: "rgba(6, 214, 160, 0.1)",
                        color: GREEN,
                      }}
                    >
                      {pipeline.step_count} step
                      {pipeline.step_count !== 1 ? "s" : ""}
                    </span>
                  </td>
                  <td className="px-4 py-3 text-xs text-slate-400 truncate max-w-xs">
                    {pipeline.description || "---"}
                  </td>
                  <td className="px-4 py-3 text-xs text-slate-500 font-mono">
                    {formatDate(pipeline.created_at)}
                  </td>
                  <td className="px-4 py-3">
                    <button
                      onClick={(e) => {
                        e.stopPropagation();
                        setDeleteTarget(pipeline);
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
          className="fixed inset-0 z-50 flex items-start justify-center pt-8"
          style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
          onClick={() => setModalOpen(false)}
        >
          <div
            className="w-full max-w-3xl max-h-[90vh] overflow-y-auto rounded-xl p-6 relative"
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
              {modalMode === "create" ? "Create Pipeline" : "Edit Pipeline"}
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
                placeholder="Security Analysis Pipeline"
                style={inputStyle}
              />
            </label>

            {/* Description */}
            <label className="block mb-5">
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
                placeholder="Brief description of what this pipeline does"
                style={inputStyle}
              />
            </label>

            {/* D3 Pipeline Diagram */}
            <div className="mb-5">
              <div
                className="text-[10px] font-bold uppercase tracking-wider mb-2"
                style={{ color: "#94a3b8" }}
              >
                Pipeline Flow
              </div>
              <div
                className="rounded-lg p-4"
                style={{
                  background: "rgba(15, 23, 42, 0.5)",
                  border: "1px solid rgba(0, 240, 255, 0.08)",
                  minHeight: 80,
                }}
              >
                <PipelineDiagram steps={form.steps} agents={agents} />
              </div>
            </div>

            {/* Steps Editor */}
            <div className="mb-5">
              <div className="flex items-center justify-between mb-3">
                <span
                  className="text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: "#94a3b8" }}
                >
                  Steps ({form.steps.length})
                </span>
                <button
                  onClick={addStep}
                  style={btnSmall}
                  disabled={agents.length === 0}
                  title={
                    agents.length === 0
                      ? "Create agents first"
                      : "Add a step"
                  }
                >
                  + Add Step
                </button>
              </div>

              {agents.length === 0 && (
                <div
                  className="text-xs font-mono mb-3"
                  style={{ color: "#f59e0b" }}
                >
                  No agents available. Create agents first.
                </div>
              )}

              {form.steps.length === 0 && agents.length > 0 && (
                <div
                  className="rounded-lg p-6 text-center"
                  style={{
                    background: "rgba(15, 23, 42, 0.4)",
                    border: "1px dashed rgba(0, 240, 255, 0.15)",
                    borderRadius: 8,
                  }}
                >
                  <div className="text-xs text-slate-500 font-mono">
                    No steps yet. Click "+ Add Step" to begin.
                  </div>
                </div>
              )}

              <div className="space-y-2">
                {form.steps.map((step, idx) => (
                  <div
                    key={idx}
                    className="rounded-lg p-3"
                    style={{
                      background:
                        editingStepIdx === idx
                          ? "rgba(0, 240, 255, 0.06)"
                          : "rgba(15, 23, 42, 0.5)",
                      border:
                        editingStepIdx === idx
                          ? `1px solid ${CYAN}33`
                          : "1px solid rgba(0, 240, 255, 0.08)",
                      borderRadius: 8,
                    }}
                  >
                    {/* Step header row */}
                    <div className="flex items-center gap-3">
                      {/* Order badge */}
                      <div
                        className="flex items-center justify-center rounded-full font-mono text-xs font-bold"
                        style={{
                          width: 24,
                          height: 24,
                          minWidth: 24,
                          background:
                            step.parallel_group !== null
                              ? "rgba(6, 214, 160, 0.15)"
                              : "rgba(0, 240, 255, 0.15)",
                          color:
                            step.parallel_group !== null ? GREEN : CYAN,
                        }}
                      >
                        {idx + 1}
                      </div>

                      {/* Agent select */}
                      <select
                        value={step.agent_id ?? ""}
                        onChange={(e) =>
                          updateStep(idx, {
                            agent_id: e.target.value || null,
                          })
                        }
                        style={{
                          ...inputStyle,
                          width: "auto",
                          flex: 1,
                          padding: "4px 8px",
                          fontSize: 12,
                          appearance:
                            "auto" as React.CSSProperties["appearance"],
                        }}
                      >
                        <option value="">(unassigned)</option>
                        {agents.map((a) => (
                          <option key={a.id} value={a.id}>
                            {a.name}
                          </option>
                        ))}
                      </select>

                      {/* Parallel group */}
                      <label
                        className="flex items-center gap-1.5"
                        style={{ minWidth: 100 }}
                      >
                        <span
                          className="text-[9px] uppercase font-bold"
                          style={{ color: "#64748b" }}
                        >
                          Group
                        </span>
                        <input
                          type="number"
                          min={0}
                          value={
                            step.parallel_group !== null
                              ? step.parallel_group
                              : ""
                          }
                          onChange={(e) => {
                            const val = e.target.value.trim();
                            updateStep(idx, {
                              parallel_group:
                                val === "" ? null : parseInt(val, 10),
                            });
                          }}
                          placeholder="--"
                          title="Parallel group number (empty = sequential)"
                          style={{
                            ...inputStyle,
                            width: 48,
                            padding: "4px 6px",
                            fontSize: 11,
                            textAlign: "center" as const,
                          }}
                        />
                      </label>

                      {/* Action buttons */}
                      <div className="flex items-center gap-1">
                        <button
                          onClick={() => moveStep(idx, "up")}
                          disabled={idx === 0}
                          title="Move up"
                          style={{
                            ...btnSmall,
                            padding: "2px 6px",
                            opacity: idx === 0 ? 0.3 : 1,
                          }}
                        >
                          &#9650;
                        </button>
                        <button
                          onClick={() => moveStep(idx, "down")}
                          disabled={idx === form.steps.length - 1}
                          title="Move down"
                          style={{
                            ...btnSmall,
                            padding: "2px 6px",
                            opacity:
                              idx === form.steps.length - 1 ? 0.3 : 1,
                          }}
                        >
                          &#9660;
                        </button>
                        <button
                          onClick={() => openStepConfig(idx)}
                          title="Configure input mapping"
                          style={{
                            ...btnSmall,
                            padding: "2px 8px",
                            color:
                              step.input_mapping !== null ? GREEN : CYAN,
                          }}
                        >
                          { "{}" }
                        </button>
                        <button
                          onClick={() => removeStep(idx)}
                          title="Remove step"
                          style={{
                            ...btnDanger,
                            padding: "2px 8px",
                            fontSize: 11,
                          }}
                        >
                          X
                        </button>
                      </div>
                    </div>

                    {/* Input mapping editor (expanded) */}
                    {editingStepIdx === idx && (
                      <div className="mt-3">
                        <div
                          className="text-[9px] uppercase font-bold tracking-wider mb-1.5"
                          style={{ color: "#94a3b8" }}
                        >
                          Input Mapping (JSON)
                        </div>
                        <textarea
                          value={inputMappingText}
                          onChange={(e) =>
                            setInputMappingText(e.target.value)
                          }
                          rows={4}
                          style={{
                            ...inputStyle,
                            resize: "vertical",
                            lineHeight: 1.5,
                            borderColor: (() => {
                              try {
                                JSON.parse(inputMappingText);
                                return "rgba(0, 240, 255, 0.15)";
                              } catch {
                                return "rgba(239, 68, 68, 0.4)";
                              }
                            })(),
                          }}
                        />
                        <div className="flex items-center gap-2 mt-2">
                          <button onClick={saveStepMapping} style={btnSmall}>
                            Save Mapping
                          </button>
                          <button
                            onClick={() => setEditingStepIdx(null)}
                            style={{
                              ...btnGhost,
                              padding: "4px 12px",
                              fontSize: 11,
                            }}
                          >
                            Cancel
                          </button>
                          {(() => {
                            try {
                              JSON.parse(inputMappingText);
                              return null;
                            } catch {
                              return (
                                <span className="text-[10px] text-red-400 font-mono">
                                  invalid JSON
                                </span>
                              );
                            }
                          })()}
                        </div>
                      </div>
                    )}
                  </div>
                ))}
              </div>
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
                    ? "Create Pipeline"
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
              Delete Pipeline
            </h3>
            <p className="text-sm text-slate-400 mb-5">
              Are you sure you want to delete{" "}
              <span className="font-semibold text-slate-200">
                {deleteTarget.name}
              </span>
              ? This will remove all steps. This action cannot be undone.
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
