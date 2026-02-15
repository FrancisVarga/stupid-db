"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { useRouter } from "next/navigation";
import {
  listReports,
  deleteReport,
  type Report,
} from "@/lib/reports";

export default function ReportsIndexPage() {
  const router = useRouter();
  const [reports, setReports] = useState<Report[]>([]);
  const [loaded, setLoaded] = useState(false);

  useEffect(() => {
    queueMicrotask(() => {
      setReports(listReports());
      setLoaded(true);
    });
  }, []);

  function handleDelete(id: string) {
    deleteReport(id);
    setReports(listReports());
  }

  function handleClearAll() {
    for (const r of reports) {
      deleteReport(r.id);
    }
    setReports([]);
  }

  if (!loaded) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="text-slate-600 text-sm animate-pulse">
          Loading reports...
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link href="/" className="hover:opacity-80 transition-opacity">
            <span
              className="text-lg font-bold tracking-wider"
              style={{ color: "#00f0ff" }}
            >
              stupid-db
            </span>
          </Link>
          <span className="text-slate-600">/</span>
          <span
            className="text-sm font-bold tracking-wider uppercase"
            style={{ color: "#06d6a0" }}
          >
            Reports
          </span>
        </div>
        <div className="flex items-center gap-3">
          {reports.length > 0 && (
            <button
              onClick={handleClearAll}
              className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg hover:opacity-90 transition-opacity"
              style={{
                color: "#ff4757",
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.12)",
              }}
            >
              Clear All
            </button>
          )}
          <Link
            href="/explore"
            className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg hover:opacity-90 transition-opacity"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.12)",
            }}
          >
            New Chat
          </Link>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-5 max-w-4xl mx-auto w-full">
        {reports.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-20">
            <div className="text-slate-600 text-sm mb-4">
              No saved reports yet
            </div>
            <p className="text-slate-700 text-xs text-center max-w-sm mb-6">
              Reports are saved conversations from the explorer. Open the
              explorer, run queries, and click &ldquo;Save Report&rdquo; to
              create shareable snapshots.
            </p>
            <Link
              href="/explore"
              className="text-xs font-bold tracking-wider uppercase px-4 py-2 rounded-lg"
              style={{
                color: "#00f0ff",
                background: "rgba(0, 240, 255, 0.08)",
                border: "1px solid rgba(0, 240, 255, 0.15)",
              }}
            >
              Open Explorer
            </Link>
          </div>
        ) : (
          <div className="space-y-3">
            <div className="text-[10px] text-slate-500 font-mono mb-2">
              {reports.length} saved report{reports.length !== 1 ? "s" : ""}
            </div>
            {reports.map((report) => {
              const messageCount = report.messages.length;
              const userMsgCount = report.messages.filter(
                (m) => m.role === "user"
              ).length;
              const hasViz = report.messages.some(
                (m) => m.renderBlocks && m.renderBlocks.length > 0
              );

              return (
                <div
                  key={report.id}
                  className="rounded-xl p-4 relative overflow-hidden group"
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
                        "linear-gradient(90deg, transparent, rgba(0, 240, 255, 0.15), transparent)",
                    }}
                  />

                  <div className="flex items-start justify-between">
                    <button
                      onClick={() => router.push(`/reports/${report.id}`)}
                      className="text-left flex-1 min-w-0"
                    >
                      <h3 className="text-sm font-bold text-slate-200 truncate hover:text-white transition-colors">
                        {report.title}
                      </h3>
                      <div className="flex items-center gap-3 mt-1">
                        <span className="text-[10px] text-slate-600 font-mono">
                          {new Date(report.created_at).toLocaleDateString(
                            undefined,
                            {
                              month: "short",
                              day: "numeric",
                              year: "numeric",
                              hour: "2-digit",
                              minute: "2-digit",
                            }
                          )}
                        </span>
                        <span className="text-[10px] text-slate-600 font-mono">
                          {messageCount} messages
                        </span>
                        <span className="text-[10px] text-slate-600 font-mono">
                          {userMsgCount} queries
                        </span>
                        {hasViz && (
                          <span
                            className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
                            style={{
                              background: "rgba(165, 85, 247, 0.1)",
                              color: "#a855f7",
                            }}
                          >
                            viz
                          </span>
                        )}
                      </div>

                      {/* Preview of first user message */}
                      {report.messages.find((m) => m.role === "user") && (
                        <p className="text-[11px] text-slate-500 mt-2 truncate">
                          &ldquo;
                          {
                            report.messages.find((m) => m.role === "user")!
                              .content
                          }
                          &rdquo;
                        </p>
                      )}
                    </button>

                    <div className="flex items-center gap-1 ml-3 shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
                      <button
                        onClick={() => router.push(`/reports/${report.id}`)}
                        className="text-[10px] font-bold tracking-wider uppercase px-2 py-1 rounded"
                        style={{
                          color: "#00f0ff",
                          background: "rgba(0, 240, 255, 0.06)",
                        }}
                      >
                        View
                      </button>
                      <button
                        onClick={() => handleDelete(report.id)}
                        className="text-[10px] font-bold tracking-wider uppercase px-2 py-1 rounded"
                        style={{
                          color: "#ff4757",
                          background: "rgba(255, 71, 87, 0.06)",
                        }}
                      >
                        Delete
                      </button>
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </div>
  );
}
