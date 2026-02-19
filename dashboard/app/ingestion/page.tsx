"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import {
  fetchIngestionSources,
  fetchIngestionJobs,
  deleteIngestionSource,
  triggerIngestionJob,
  type IngestionSource,
  type IngestionJob,
} from "@/lib/api";

type Tab = "sources" | "jobs";

const SOURCE_TYPE_COLORS: Record<IngestionSource["source_type"], string> = {
  parquet: "bg-blue-500/20 text-blue-400 border-blue-500/30",
  directory: "bg-green-500/20 text-green-400 border-green-500/30",
  s3: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30",
  csv_json: "bg-purple-500/20 text-purple-400 border-purple-500/30",
  push: "bg-teal-500/20 text-teal-400 border-teal-500/30",
  queue: "bg-orange-500/20 text-orange-400 border-orange-500/30",
};

const TRIGGER_KIND_COLORS: Record<IngestionJob["trigger_kind"], string> = {
  manual: "bg-slate-500/20 text-slate-400 border-slate-500/30",
  scheduled: "bg-blue-500/20 text-blue-400 border-blue-500/30",
  push: "bg-teal-500/20 text-teal-400 border-teal-500/30",
  watch: "bg-green-500/20 text-green-400 border-green-500/30",
};

const STATUS_COLORS: Record<IngestionJob["status"], string> = {
  pending: "bg-slate-500/20 text-slate-400 border-slate-500/30",
  running: "bg-yellow-500/20 text-yellow-400 border-yellow-500/30",
  completed: "bg-green-500/20 text-green-400 border-green-500/30",
  failed: "bg-red-500/20 text-red-400 border-red-500/30",
  cancelled: "bg-slate-500/20 text-slate-400 border-slate-500/30",
};

function relativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const secs = Math.floor(diff / 1000);
  if (secs < 60) return `${secs}s ago`;
  const mins = Math.floor(secs / 60);
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.floor(hrs / 24);
  return `${days}d ago`;
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const secs = (ms / 1000).toFixed(1);
  return `${secs}s`;
}

// ── Sources Tab ───────────────────────────────────────────────

function SourcesTable({
  sources,
  loading,
  onTrigger,
  onDelete,
}: {
  sources: IngestionSource[];
  loading: boolean;
  onTrigger: (id: string) => void;
  onDelete: (id: string, name: string) => void;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-20 text-slate-500 text-sm">
        Loading sources...
      </div>
    );
  }

  if (sources.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-20 text-slate-500 text-sm gap-2">
        <span>No sources configured.</span>
        <span>
          Add one via the{" "}
          <Link href="/ingestion/new" className="text-teal-400 hover:underline">
            + New Source
          </Link>{" "}
          button.
        </span>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-[10px] uppercase tracking-wider text-slate-500 border-b border-white/5">
            <th className="py-2 pr-4">Name</th>
            <th className="py-2 pr-4">Type</th>
            <th className="py-2 pr-4">ZMQ</th>
            <th className="py-2 pr-4">Schedule</th>
            <th className="py-2 pr-4">Enabled</th>
            <th className="py-2 pr-4">Last Run</th>
            <th className="py-2 text-right">Actions</th>
          </tr>
        </thead>
        <tbody>
          {sources.map((s) => (
            <tr
              key={s.id}
              className="border-b border-white/5 hover:bg-white/5 transition-colors"
            >
              <td className="py-3 pr-4 font-semibold text-[#e0e6f0]">
                {s.name}
              </td>
              <td className="py-3 pr-4">
                <span
                  className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium border ${SOURCE_TYPE_COLORS[s.source_type]}`}
                >
                  {s.source_type}
                </span>
              </td>
              <td className="py-3 pr-4 text-slate-500 text-xs">
                {s.zmq_granularity}
              </td>
              <td className="py-3 pr-4 text-xs font-mono text-slate-400">
                {s.schedule_json?.cron ?? "\u2014"}
              </td>
              <td className="py-3 pr-4">
                <span
                  className={`inline-block w-2 h-2 rounded-full ${
                    s.enabled ? "bg-green-400" : "bg-slate-600"
                  }`}
                  title={s.enabled ? "Enabled" : "Disabled"}
                />
              </td>
              <td className="py-3 pr-4 text-xs text-slate-400">
                {s.last_run_at ? relativeTime(s.last_run_at) : "Never"}
              </td>
              <td className="py-3 text-right">
                <div className="flex items-center justify-end gap-2">
                  <button
                    onClick={() => onTrigger(s.id)}
                    className="rounded px-3 py-1.5 text-xs font-medium bg-teal-500/15 text-teal-400 border border-teal-500/30 hover:bg-teal-500/25 transition-colors"
                  >
                    Trigger
                  </button>
                  <button
                    onClick={() => onDelete(s.id, s.name)}
                    className="rounded px-3 py-1.5 text-xs font-medium bg-red-500/10 text-red-400 border border-red-500/20 hover:bg-red-500/20 transition-colors"
                  >
                    Delete
                  </button>
                </div>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

// ── Jobs Tab ──────────────────────────────────────────────────

function JobsList({
  jobs,
  loading,
}: {
  jobs: IngestionJob[];
  loading: boolean;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-20 text-slate-500 text-sm">
        Loading jobs...
      </div>
    );
  }

  if (jobs.length === 0) {
    return (
      <div className="flex flex-col items-center justify-center py-20 text-slate-500 text-sm gap-2">
        <span>No jobs yet.</span>
        <span>Trigger a source from the Sources tab.</span>
      </div>
    );
  }

  return (
    <div className="overflow-x-auto">
      <table className="w-full text-sm">
        <thead>
          <tr className="text-left text-[10px] uppercase tracking-wider text-slate-500 border-b border-white/5">
            <th className="py-2 pr-4">Job ID</th>
            <th className="py-2 pr-4">Source</th>
            <th className="py-2 pr-4">Trigger</th>
            <th className="py-2 pr-4">Status</th>
            <th className="py-2 pr-4">Progress</th>
            <th className="py-2 pr-4">Duration</th>
            <th className="py-2">Created</th>
          </tr>
        </thead>
        <tbody>
          {jobs.map((j) => {
            const docsProcessed = j.docs_processed ?? 0;
            const docsTotal = j.docs_total ?? 0;
            const pct =
              docsTotal > 0
                ? Math.round((docsProcessed / docsTotal) * 100)
                : 0;

            let duration = "\u2014";
            if (j.duration_ms != null) {
              duration = formatDuration(j.duration_ms);
            } else if (j.status === "running") {
              duration = "running\u2026";
            }

            return (
              <tr
                key={j.id}
                className="border-b border-white/5 hover:bg-white/5 transition-colors"
              >
                <td className="py-3 pr-4 font-mono text-xs text-slate-400">
                  {j.id.slice(0, 8)}
                </td>
                <td className="py-3 pr-4 text-[#e0e6f0]">{j.source_name}</td>
                <td className="py-3 pr-4">
                  <span
                    className={`inline-block rounded-full px-2 py-0.5 text-xs font-medium border ${TRIGGER_KIND_COLORS[j.trigger_kind]}`}
                  >
                    {j.trigger_kind}
                  </span>
                </td>
                <td className="py-3 pr-4">
                  <span
                    className={`inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium border ${STATUS_COLORS[j.status]}`}
                  >
                    {j.status === "running" && (
                      <span className="inline-block w-1.5 h-1.5 rounded-full bg-yellow-400 animate-pulse" />
                    )}
                    {j.status}
                  </span>
                </td>
                <td className="py-3 pr-4">
                  {j.status === "running" || docsTotal > 0 ? (
                    <div className="flex items-center gap-2 min-w-[120px]">
                      <div className="flex-1 h-1.5 rounded-full bg-white/5 overflow-hidden">
                        <div
                          className="h-full rounded-full bg-teal-500 transition-all duration-300"
                          style={{ width: `${pct}%` }}
                        />
                      </div>
                      <span className="text-[10px] text-slate-500 font-mono whitespace-nowrap">
                        {docsProcessed}/{docsTotal}
                      </span>
                    </div>
                  ) : (
                    <span className="text-slate-600 text-xs">\u2014</span>
                  )}
                </td>
                <td className="py-3 pr-4 text-xs text-slate-400 font-mono">
                  {duration}
                </td>
                <td className="py-3 text-xs text-slate-400">
                  {relativeTime(j.created_at)}
                </td>
              </tr>
            );
          })}
        </tbody>
      </table>
    </div>
  );
}

// ── Main Page ─────────────────────────────────────────────────

export default function IngestionPage() {
  const [tab, setTab] = useState<Tab>("sources");
  const [sources, setSources] = useState<IngestionSource[]>([]);
  const [jobs, setJobs] = useState<IngestionJob[]>([]);
  const [loading, setLoading] = useState(true);
  const [refreshKey, setRefreshKey] = useState(0);
  const [error, setError] = useState<string | null>(null);

  const loadAll = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const [s, j] = await Promise.all([
        fetchIngestionSources(),
        fetchIngestionJobs(),
      ]);
      setSources(s);
      setJobs(j);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load data");
    } finally {
      setLoading(false);
    }
  }, []);

  // Load on mount and on refreshKey change
  useEffect(() => {
    loadAll();
  }, [refreshKey, loadAll]);

  // Auto-refresh jobs every 3s if any are running/pending
  useEffect(() => {
    const hasActive = jobs.some(
      (j) => j.status === "running" || j.status === "pending",
    );
    if (!hasActive) return;
    const timer = setInterval(() => {
      fetchIngestionJobs().then(setJobs).catch(() => {});
    }, 3000);
    return () => clearInterval(timer);
  }, [jobs]);

  const handleTrigger = async (sourceId: string) => {
    try {
      await triggerIngestionJob(sourceId);
      setRefreshKey((k) => k + 1);
      setTab("jobs");
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Failed to trigger job",
      );
    }
  };

  const handleDelete = async (sourceId: string, sourceName: string) => {
    if (!confirm(`Delete source "${sourceName}"? This cannot be undone.`)) {
      return;
    }
    try {
      await deleteIngestionSource(sourceId);
      setRefreshKey((k) => k + 1);
    } catch (e) {
      setError(
        e instanceof Error ? e.message : "Failed to delete source",
      );
    }
  };

  const tabs: { key: Tab; label: string; count: number }[] = [
    { key: "sources", label: "Sources", count: sources.length },
    { key: "jobs", label: "Jobs", count: jobs.length },
  ];

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(20, 184, 166, 0.08)",
          background:
            "linear-gradient(180deg, rgba(20, 184, 166, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-xs"
          >
            &larr; Dashboard
          </Link>
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#14b8a6" }}
          >
            Ingestion
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            Data Sources & Jobs
          </span>
        </div>
        <Link
          href="/ingestion/new"
          className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
          style={{
            background: "rgba(20, 184, 166, 0.12)",
            border: "1px solid rgba(20, 184, 166, 0.3)",
            color: "#14b8a6",
          }}
        >
          + New Source
        </Link>
      </header>

      {/* Tab bar */}
      <div
        className="px-6 flex items-center gap-1"
        style={{ borderBottom: "1px solid rgba(20, 184, 166, 0.06)" }}
      >
        {tabs.map((t) => {
          const isActive = tab === t.key;
          return (
            <button
              key={t.key}
              onClick={() => setTab(t.key)}
              className="px-4 py-2.5 text-xs font-medium tracking-wide transition-colors relative"
              style={{ color: isActive ? "#14b8a6" : "#64748b" }}
            >
              {t.label}
              {!loading && (
                <span className="ml-1.5 text-[10px] opacity-60">
                  {t.count}
                </span>
              )}
              {isActive && (
                <span
                  className="absolute bottom-0 left-0 w-full h-[2px]"
                  style={{ background: "#14b8a6" }}
                />
              )}
            </button>
          );
        })}
      </div>

      {/* Error banner */}
      {error && (
        <div className="mx-6 mt-3 px-4 py-2 rounded-lg bg-red-500/10 border border-red-500/20 text-red-400 text-xs flex items-center justify-between">
          <span>{error}</span>
          <button
            onClick={() => setError(null)}
            className="text-red-400/60 hover:text-red-400 ml-4"
          >
            &times;
          </button>
        </div>
      )}

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-4">
        {tab === "sources" && (
          <SourcesTable
            sources={sources}
            loading={loading}
            onTrigger={handleTrigger}
            onDelete={handleDelete}
          />
        )}
        {tab === "jobs" && <JobsList jobs={jobs} loading={loading} />}
      </div>
    </div>
  );
}
