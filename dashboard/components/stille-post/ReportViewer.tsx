"use client";

import { useState, useEffect, useCallback } from "react";
import RenderBlockView from "@/components/chat/RenderBlockView";
import type { RenderBlock } from "@/lib/reports";

// ── Types ───────────────────────────────────────────────────────

interface SpReport {
  id: string;
  run_id: string | null;
  title: string;
  content_html: string | null;
  content_json: unknown | null;
  render_blocks: RenderBlock[] | null;
  created_at: string;
}

// ── Subcomponents ───────────────────────────────────────────────

function ReportList({
  reports,
  loading,
  error,
  onSelect,
}: {
  reports: SpReport[];
  loading: boolean;
  error: string | null;
  onSelect: (r: SpReport) => void;
}) {
  if (loading) {
    return (
      <div className="flex items-center justify-center py-16">
        <div
          className="w-5 h-5 border-2 rounded-full animate-spin"
          style={{
            borderColor: "rgba(0, 240, 255, 0.15)",
            borderTopColor: "#00f0ff",
          }}
        />
        <span className="ml-3 text-xs text-slate-500 font-mono">
          Loading reports...
        </span>
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="rounded-xl p-6 text-center"
        style={{
          background: "rgba(255, 71, 87, 0.05)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
        }}
      >
        <div className="text-xs text-red-400 font-mono">{error}</div>
      </div>
    );
  }

  if (reports.length === 0) {
    return (
      <div
        className="rounded-xl p-8 text-center"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: "1px solid rgba(0, 240, 255, 0.1)",
        }}
      >
        <div
          className="text-[10px] font-bold uppercase tracking-[0.15em] mb-2"
          style={{ color: "#06d6a0" }}
        >
          No Reports
        </div>
        <p className="text-sm text-slate-500 font-mono">
          Reports will appear here after pipeline runs complete.
        </p>
      </div>
    );
  }

  return (
    <div className="overflow-auto">
      <table className="w-full text-[11px] font-mono">
        <thead>
          <tr style={{ borderBottom: "1px solid #1e293b" }}>
            <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
              Title
            </th>
            <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
              Run ID
            </th>
            <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
              Blocks
            </th>
            <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
              Created
            </th>
          </tr>
        </thead>
        <tbody>
          {reports.map((r) => (
            <tr
              key={r.id}
              onClick={() => onSelect(r)}
              className="cursor-pointer transition-colors"
              style={{ borderBottom: "1px solid #0f172a" }}
              onMouseEnter={(e) =>
                ((e.currentTarget as HTMLElement).style.background =
                  "rgba(0, 240, 255, 0.03)")
              }
              onMouseLeave={(e) =>
                ((e.currentTarget as HTMLElement).style.background =
                  "transparent")
              }
            >
              <td className="px-3 py-2 text-slate-300">{r.title}</td>
              <td className="px-3 py-2 text-slate-500">
                {r.run_id ? r.run_id.slice(0, 8) : "-"}
              </td>
              <td className="px-3 py-2">
                <span
                  className="text-[10px] font-bold px-1.5 py-0.5 rounded"
                  style={{
                    color: r.render_blocks?.length ? "#06d6a0" : "#475569",
                    background: r.render_blocks?.length
                      ? "rgba(6, 214, 160, 0.08)"
                      : "transparent",
                  }}
                >
                  {r.render_blocks?.length || 0}
                </span>
              </td>
              <td className="px-3 py-2 text-slate-500 whitespace-nowrap">
                {new Date(r.created_at).toLocaleString()}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

function ReportDetail({
  report,
  onBack,
}: {
  report: SpReport;
  onBack: () => void;
}) {
  const blocks = report.render_blocks;
  const hasBlocks = blocks && blocks.length > 0;

  return (
    <div className="space-y-4">
      {/* Header */}
      <div className="flex items-center gap-3">
        <button
          onClick={onBack}
          className="text-slate-500 hover:text-slate-300 text-xs font-mono transition-colors"
        >
          &larr; Back
        </button>
        <div
          className="w-[1px] h-4"
          style={{ background: "rgba(0, 240, 255, 0.12)" }}
        />
        <h2
          className="text-sm font-bold tracking-wider"
          style={{ color: "#00f0ff" }}
        >
          {report.title}
        </h2>
        <span className="text-[10px] text-slate-600 font-mono ml-auto">
          {new Date(report.created_at).toLocaleString()}
        </span>
      </div>

      {/* Render blocks */}
      {hasBlocks ? (
        <div className="space-y-4">
          {blocks.map((block, i) => (
            <div
              key={i}
              className="rounded-xl overflow-hidden relative"
              style={{
                background:
                  "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                border: "1px solid rgba(0, 240, 255, 0.08)",
              }}
            >
              <div
                className="absolute top-0 left-0 w-full h-[1px]"
                style={{
                  background:
                    "linear-gradient(90deg, transparent, rgba(0, 240, 255, 0.3), transparent)",
                }}
              />
              <div className="p-4">
                <RenderBlockView block={block} />
              </div>
            </div>
          ))}
        </div>
      ) : report.content_html ? (
        /* HTML fallback */
        <div
          className="rounded-xl p-6 prose prose-invert prose-sm max-w-none"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
          dangerouslySetInnerHTML={{ __html: report.content_html }}
        />
      ) : (
        <div
          className="rounded-xl p-6 text-center"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <p className="text-sm text-slate-500 font-mono">
            This report has no visual content.
          </p>
        </div>
      )}
    </div>
  );
}

// ── Main Component ──────────────────────────────────────────────

export default function ReportViewer({
  refreshKey,
}: {
  refreshKey?: number;
}) {
  const [reports, setReports] = useState<SpReport[]>([]);
  const [selected, setSelected] = useState<SpReport | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const res = await fetch("/api/stille-post/reports");
      if (!res.ok) throw new Error(`Failed to load reports (${res.status})`);
      const data: SpReport[] = await res.json();
      setReports(data);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : "Unknown error");
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load, refreshKey]);

  if (selected) {
    return <ReportDetail report={selected} onBack={() => setSelected(null)} />;
  }

  return (
    <ReportList
      reports={reports}
      loading={loading}
      error={error}
      onSelect={setSelected}
    />
  );
}
