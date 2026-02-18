"use client";

import { useState, useEffect, useCallback, useRef } from "react";

/* ── Types ───────────────────────────────────── */

interface StepResult {
  id: string;
  run_id: string;
  step_id: string;
  agent_id: string;
  input_data: unknown;
  output_data: unknown;
  tokens_used: number;
  duration_ms: number;
  status: string;
}

interface Run {
  id: string;
  pipeline_id: string;
  schedule_id: string | null;
  status: "pending" | "running" | "completed" | "failed" | "cancelled";
  started_at: string | null;
  completed_at: string | null;
  error: string | null;
  trigger_type: "manual" | "scheduled" | "event";
  created_at: string;
  step_results?: StepResult[];
}

interface Pipeline {
  id: string;
  name: string;
}

/* ── Constants ───────────────────────────────── */

const STATUS_COLORS: Record<string, string> = {
  pending: "#eab308",
  running: "#00f0ff",
  completed: "#06d6a0",
  failed: "#ef4444",
  cancelled: "#64748b",
};

const ALL_STATUSES = ["pending", "running", "completed", "failed", "cancelled"] as const;

const POLL_INTERVAL = 5000;

/* ── Helpers ─────────────────────────────────── */

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
  const mins = Math.floor(ms / 60_000);
  const secs = Math.floor((ms % 60_000) / 1000);
  return `${mins}m ${secs}s`;
}

function formatDate(iso: string): string {
  const d = new Date(iso);
  return d.toLocaleString("en-US", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function computeDuration(run: Run): string {
  if (!run.started_at) return "—";
  const start = new Date(run.started_at).getTime();
  const end = run.completed_at ? new Date(run.completed_at).getTime() : Date.now();
  return formatDuration(end - start);
}

/* ── Component ───────────────────────────────── */

export default function RunHistory() {
  const [runs, setRuns] = useState<Run[]>([]);
  const [pipelines, setPipelines] = useState<Pipeline[]>([]);
  const [loading, setLoading] = useState(true);
  const [statusFilter, setStatusFilter] = useState<string | null>(null);
  const [expandedRunId, setExpandedRunId] = useState<string | null>(null);
  const [expandedRun, setExpandedRun] = useState<Run | null>(null);
  const [triggerPipelineId, setTriggerPipelineId] = useState<string>("");
  const [triggering, setTriggering] = useState(false);
  const pollRef = useRef<ReturnType<typeof setInterval> | null>(null);

  /* Fetch runs list */
  const fetchRuns = useCallback(async () => {
    try {
      const params = new URLSearchParams();
      if (statusFilter) params.set("status", statusFilter);
      const res = await fetch(`/api/stille-post/runs?${params}`);
      if (res.ok) {
        const data = await res.json();
        setRuns(Array.isArray(data) ? data : data.runs ?? []);
      }
    } catch {
      /* swallow — will retry on next poll */
    } finally {
      setLoading(false);
    }
  }, [statusFilter]);

  /* Fetch pipelines for trigger dropdown */
  useEffect(() => {
    fetch("/api/stille-post/pipelines")
      .then((r) => (r.ok ? r.json() : []))
      .then((d) => setPipelines(Array.isArray(d) ? d : d.pipelines ?? []))
      .catch(() => {});
  }, []);

  /* Initial fetch + polling */
  useEffect(() => {
    setLoading(true);
    fetchRuns();

    const hasActive = runs.some((r) => r.status === "pending" || r.status === "running");
    if (hasActive) {
      pollRef.current = setInterval(fetchRuns, POLL_INTERVAL);
    }
    return () => {
      if (pollRef.current) clearInterval(pollRef.current);
    };
  }, [fetchRuns]); // eslint-disable-line react-hooks/exhaustive-deps

  /* Start/stop polling based on active runs */
  useEffect(() => {
    const hasActive = runs.some((r) => r.status === "pending" || r.status === "running");
    if (hasActive && !pollRef.current) {
      pollRef.current = setInterval(fetchRuns, POLL_INTERVAL);
    } else if (!hasActive && pollRef.current) {
      clearInterval(pollRef.current);
      pollRef.current = null;
    }
  }, [runs, fetchRuns]);

  /* Fetch run detail (with step results) */
  const toggleExpand = useCallback(
    async (runId: string) => {
      if (expandedRunId === runId) {
        setExpandedRunId(null);
        setExpandedRun(null);
        return;
      }
      setExpandedRunId(runId);
      setExpandedRun(null);
      try {
        const res = await fetch(`/api/stille-post/runs/${runId}`);
        if (res.ok) {
          setExpandedRun(await res.json());
        }
      } catch {
        /* ignore */
      }
    },
    [expandedRunId],
  );

  /* Trigger manual run */
  const triggerRun = useCallback(async () => {
    if (!triggerPipelineId || triggering) return;
    setTriggering(true);
    try {
      const res = await fetch("/api/stille-post/runs", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ pipeline_id: triggerPipelineId }),
      });
      if (res.ok) {
        setTriggerPipelineId("");
        fetchRuns();
      }
    } catch {
      /* ignore */
    } finally {
      setTriggering(false);
    }
  }, [triggerPipelineId, triggering, fetchRuns]);

  /* Pipeline name lookup */
  const pipelineName = useCallback(
    (id: string) => pipelines.find((p) => p.id === id)?.name ?? id.slice(0, 8),
    [pipelines],
  );

  return (
    <div className="space-y-4">
      {/* ── Toolbar ───────────────────────────── */}
      <div className="flex items-center justify-between gap-4 flex-wrap">
        {/* Status filters */}
        <div className="flex items-center gap-1.5">
          <button
            onClick={() => setStatusFilter(null)}
            className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider rounded transition-colors"
            style={{
              background: statusFilter === null ? "rgba(0, 240, 255, 0.12)" : "transparent",
              color: statusFilter === null ? "#00f0ff" : "#475569",
              border: `1px solid ${statusFilter === null ? "rgba(0, 240, 255, 0.3)" : "rgba(71, 85, 105, 0.3)"}`,
            }}
          >
            All
          </button>
          {ALL_STATUSES.map((s) => (
            <button
              key={s}
              onClick={() => setStatusFilter(statusFilter === s ? null : s)}
              className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider rounded transition-colors"
              style={{
                background: statusFilter === s ? `${STATUS_COLORS[s]}18` : "transparent",
                color: statusFilter === s ? STATUS_COLORS[s] : "#475569",
                border: `1px solid ${statusFilter === s ? `${STATUS_COLORS[s]}50` : "rgba(71, 85, 105, 0.3)"}`,
              }}
            >
              {s}
            </button>
          ))}
        </div>

        {/* Trigger run */}
        <div className="flex items-center gap-2">
          <select
            value={triggerPipelineId}
            onChange={(e) => setTriggerPipelineId(e.target.value)}
            className="text-xs font-mono px-2 py-1 rounded"
            style={{
              background: "rgba(15, 23, 42, 0.8)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
              color: "#94a3b8",
            }}
          >
            <option value="">Select pipeline…</option>
            {pipelines.map((p) => (
              <option key={p.id} value={p.id}>
                {p.name}
              </option>
            ))}
          </select>
          <button
            onClick={triggerRun}
            disabled={!triggerPipelineId || triggering}
            className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider rounded transition-all"
            style={{
              background:
                triggerPipelineId && !triggering
                  ? "rgba(6, 214, 160, 0.15)"
                  : "rgba(30, 41, 59, 0.5)",
              color: triggerPipelineId && !triggering ? "#06d6a0" : "#334155",
              border: `1px solid ${triggerPipelineId && !triggering ? "rgba(6, 214, 160, 0.4)" : "rgba(51, 65, 85, 0.3)"}`,
              cursor: triggerPipelineId && !triggering ? "pointer" : "not-allowed",
            }}
          >
            {triggering ? "Triggering…" : "▶ Trigger Run"}
          </button>
        </div>
      </div>

      {/* ── Table ─────────────────────────────── */}
      <div
        className="rounded-lg overflow-hidden"
        style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
      >
        <table className="w-full text-xs font-mono">
          <thead>
            <tr
              style={{
                background: "rgba(0, 240, 255, 0.03)",
                borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
              }}
            >
              {["Pipeline", "Status", "Trigger", "Duration", "Created"].map((h) => (
                <th
                  key={h}
                  className="px-4 py-2 text-left text-[10px] font-bold uppercase tracking-wider"
                  style={{ color: "#475569" }}
                >
                  {h}
                </th>
              ))}
              <th className="px-4 py-2 w-8" />
            </tr>
          </thead>
          <tbody>
            {loading && runs.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-center text-slate-600">
                  Loading runs…
                </td>
              </tr>
            ) : runs.length === 0 ? (
              <tr>
                <td colSpan={6} className="px-4 py-8 text-center text-slate-600">
                  No runs found{statusFilter ? ` with status "${statusFilter}"` : ""}
                </td>
              </tr>
            ) : (
              runs.map((run) => (
                <RunRow
                  key={run.id}
                  run={run}
                  pipelineName={pipelineName(run.pipeline_id)}
                  isExpanded={expandedRunId === run.id}
                  expandedRun={expandedRunId === run.id ? expandedRun : null}
                  onToggle={() => toggleExpand(run.id)}
                />
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* Active poll indicator */}
      {runs.some((r) => r.status === "pending" || r.status === "running") && (
        <div className="flex items-center gap-2 text-[10px] text-slate-600">
          <span
            className="inline-block w-1.5 h-1.5 rounded-full animate-pulse"
            style={{ background: "#00f0ff" }}
          />
          Auto-refreshing every {POLL_INTERVAL / 1000}s
        </div>
      )}
    </div>
  );
}

/* ── RunRow ───────────────────────────────────── */

function RunRow({
  run,
  pipelineName,
  isExpanded,
  expandedRun,
  onToggle,
}: {
  run: Run;
  pipelineName: string;
  isExpanded: boolean;
  expandedRun: Run | null;
  onToggle: () => void;
}) {
  return (
    <>
      <tr
        onClick={onToggle}
        className="cursor-pointer transition-colors"
        style={{
          background: isExpanded ? "rgba(0, 240, 255, 0.04)" : "transparent",
          borderBottom: isExpanded ? "none" : "1px solid rgba(0, 240, 255, 0.04)",
        }}
        onMouseEnter={(e) => {
          if (!isExpanded) e.currentTarget.style.background = "rgba(0, 240, 255, 0.02)";
        }}
        onMouseLeave={(e) => {
          if (!isExpanded) e.currentTarget.style.background = "transparent";
        }}
      >
        <td className="px-4 py-2.5 text-slate-300">{pipelineName}</td>
        <td className="px-4 py-2.5">
          <StatusBadge status={run.status} />
        </td>
        <td className="px-4 py-2.5">
          <TriggerBadge type={run.trigger_type} />
        </td>
        <td className="px-4 py-2.5 text-slate-400">{computeDuration(run)}</td>
        <td className="px-4 py-2.5 text-slate-500">{formatDate(run.created_at)}</td>
        <td className="px-4 py-2.5 text-slate-600">
          <span
            className="inline-block transition-transform"
            style={{ transform: isExpanded ? "rotate(90deg)" : "none" }}
          >
            ▸
          </span>
        </td>
      </tr>
      {isExpanded && (
        <tr>
          <td
            colSpan={6}
            className="px-4 pb-4"
            style={{
              background: "rgba(0, 240, 255, 0.04)",
              borderBottom: "1px solid rgba(0, 240, 255, 0.04)",
            }}
          >
            <RunDetail run={run} expandedRun={expandedRun} />
          </td>
        </tr>
      )}
    </>
  );
}

/* ── RunDetail ────────────────────────────────── */

function RunDetail({ run, expandedRun }: { run: Run; expandedRun: Run | null }) {
  if (!expandedRun) {
    return (
      <div className="py-4 text-center text-slate-600 text-[10px]">Loading step results…</div>
    );
  }

  const steps = expandedRun.step_results ?? [];

  return (
    <div className="space-y-3 pt-2">
      {/* Error banner */}
      {run.error && (
        <div
          className="rounded px-3 py-2 text-xs"
          style={{
            background: "rgba(239, 68, 68, 0.08)",
            border: "1px solid rgba(239, 68, 68, 0.2)",
            color: "#fca5a5",
          }}
        >
          <span className="font-bold">Error:</span> {run.error}
        </div>
      )}

      {/* Step timeline */}
      {steps.length === 0 ? (
        <div className="text-xs text-slate-600 py-2">No step results yet.</div>
      ) : (
        <div className="space-y-2">
          <div
            className="text-[10px] font-bold uppercase tracking-wider mb-1"
            style={{ color: "#475569" }}
          >
            Steps ({steps.length})
          </div>
          {steps.map((step, i) => (
            <StepCard key={step.id} step={step} index={i} />
          ))}
        </div>
      )}
    </div>
  );
}

/* ── StepCard ─────────────────────────────────── */

function StepCard({ step, index }: { step: StepResult; index: number }) {
  const [showInput, setShowInput] = useState(false);
  const [showOutput, setShowOutput] = useState(false);

  return (
    <div
      className="rounded-lg px-4 py-3"
      style={{
        background: "rgba(15, 23, 42, 0.6)",
        border: "1px solid rgba(0, 240, 255, 0.06)",
      }}
    >
      {/* Step header */}
      <div className="flex items-center justify-between gap-4">
        <div className="flex items-center gap-3">
          <span
            className="text-[10px] font-bold w-5 h-5 rounded flex items-center justify-center"
            style={{
              background: "rgba(0, 240, 255, 0.08)",
              color: "#00f0ff",
            }}
          >
            {index + 1}
          </span>
          <span className="text-xs text-slate-300">
            {step.agent_id ? step.agent_id.slice(0, 12) : step.step_id.slice(0, 12)}
          </span>
          <StatusBadge status={step.status} />
        </div>
        <div className="flex items-center gap-4 text-[10px] text-slate-500">
          {step.duration_ms > 0 && <span>{formatDuration(step.duration_ms)}</span>}
          {step.tokens_used > 0 && <span>{step.tokens_used.toLocaleString()} tok</span>}
        </div>
      </div>

      {/* Collapsible IO */}
      <div className="flex gap-2 mt-2">
        {step.input_data && (
          <button
            onClick={() => setShowInput(!showInput)}
            className="text-[10px] uppercase tracking-wider transition-colors"
            style={{ color: showInput ? "#00f0ff" : "#475569" }}
          >
            {showInput ? "▾" : "▸"} Input
          </button>
        )}
        {step.output_data && (
          <button
            onClick={() => setShowOutput(!showOutput)}
            className="text-[10px] uppercase tracking-wider transition-colors"
            style={{ color: showOutput ? "#06d6a0" : "#475569" }}
          >
            {showOutput ? "▾" : "▸"} Output
          </button>
        )}
      </div>

      {showInput && step.input_data && (
        <JsonViewer data={step.input_data} accent="#00f0ff" />
      )}
      {showOutput && step.output_data && (
        <JsonViewer data={step.output_data} accent="#06d6a0" />
      )}
    </div>
  );
}

/* ── JsonViewer ───────────────────────────────── */

function JsonViewer({ data, accent }: { data: unknown; accent: string }) {
  const text = typeof data === "string" ? data : JSON.stringify(data, null, 2);
  return (
    <pre
      className="mt-2 rounded px-3 py-2 text-[10px] leading-relaxed overflow-x-auto max-h-48"
      style={{
        background: "rgba(0, 0, 0, 0.3)",
        border: `1px solid ${accent}15`,
        color: "#94a3b8",
      }}
    >
      {text}
    </pre>
  );
}

/* ── Badges ───────────────────────────────────── */

function StatusBadge({ status }: { status: string }) {
  const color = STATUS_COLORS[status] ?? "#64748b";
  return (
    <span
      className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-bold uppercase tracking-wider"
      style={{
        background: `${color}15`,
        color,
        border: `1px solid ${color}30`,
      }}
    >
      {status === "running" && (
        <span className="inline-block w-1 h-1 rounded-full animate-pulse" style={{ background: color }} />
      )}
      {status}
    </span>
  );
}

function TriggerBadge({ type }: { type: string }) {
  const icons: Record<string, string> = { manual: "⚡", scheduled: "⏰", event: "📡" };
  return (
    <span className="text-[10px] text-slate-500">
      {icons[type] ?? "•"} {type}
    </span>
  );
}
