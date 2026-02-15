"use client";

import { useEffect, useState } from "react";
import { useParams, useRouter } from "next/navigation";
import { getReport, deleteReport, type Report } from "@/lib/reports";
import RenderBlockView from "@/components/chat/RenderBlockView";
import { exportCSV } from "@/lib/export";

export default function ReportPage() {
  const params = useParams();
  const router = useRouter();
  const [report, setReport] = useState<Report | null>(null);
  const [notFound, setNotFound] = useState(false);

  useEffect(() => {
    const id = params.id as string;
    const r = getReport(id);
    if (r) {
      queueMicrotask(() => setReport(r));
    } else {
      queueMicrotask(() => setNotFound(true));
    }
  }, [params.id]);

  if (notFound) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div
          className="rounded-xl p-8 max-w-md text-center"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(100, 116, 139, 0.15)",
          }}
        >
          <h2 className="text-slate-400 font-bold text-lg">Report Not Found</h2>
          <p className="text-slate-600 mt-2 text-sm">
            This report may have been deleted or the link is invalid.
          </p>
          <button
            onClick={() => router.push("/")}
            className="mt-4 text-xs font-bold tracking-wider uppercase px-4 py-2 rounded-lg"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
            }}
          >
            Back to Dashboard
          </button>
        </div>
      </div>
    );
  }

  if (!report) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div className="text-slate-600 text-sm animate-pulse">
          Loading report...
        </div>
      </div>
    );
  }

  const handleDelete = () => {
    deleteReport(report.id);
    router.push("/");
  };

  const handleExportCSV = () => {
    // Export all system messages as a simple table
    const rows = report.messages.map((m) => ({
      role: m.role,
      content: m.content,
      timestamp: m.timestamp,
    }));
    exportCSV(rows, `report-${report.id.slice(0, 8)}`);
  };

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
        <div>
          <div className="flex items-center gap-3">
            <button
              onClick={() => router.push("/")}
              className="text-slate-600 hover:text-slate-400 text-sm transition-colors"
            >
              &larr;
            </button>
            <h1 className="text-sm font-bold text-slate-200 truncate max-w-md">
              {report.title}
            </h1>
          </div>
          <div className="text-[10px] text-slate-600 font-mono mt-0.5 ml-6">
            {new Date(report.created_at).toLocaleString()}
          </div>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={() => router.push("/explore")}
            className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.12)",
            }}
          >
            Continue Chat
          </button>
          <button
            onClick={handleExportCSV}
            className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg"
            style={{
              color: "#06d6a0",
              background: "rgba(6, 214, 160, 0.06)",
              border: "1px solid rgba(6, 214, 160, 0.12)",
            }}
          >
            Export CSV
          </button>
          <button
            onClick={handleDelete}
            className="text-[10px] font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg"
            style={{
              color: "#ff4757",
              background: "rgba(255, 71, 87, 0.06)",
              border: "1px solid rgba(255, 71, 87, 0.12)",
            }}
          >
            Delete
          </button>
        </div>
      </header>

      {/* Conversation */}
      <div className="flex-1 overflow-y-auto px-6 py-4 max-w-4xl mx-auto w-full">
        <div className="space-y-4">
          {report.messages.map((msg) => (
            <div
              key={msg.id}
              className={`flex ${
                msg.role === "user" ? "justify-end" : "justify-start"
              }`}
            >
              <div
                className={`max-w-[85%] rounded-xl px-4 py-3 text-sm leading-relaxed ${
                  msg.role === "user"
                    ? "chat-bubble-user"
                    : "chat-bubble-system"
                }`}
              >
                <div className="whitespace-pre-wrap">{msg.content}</div>

                {/* Render blocks */}
                {msg.renderBlocks && msg.renderBlocks.length > 0 && (
                  <div className="mt-3 space-y-3">
                    {msg.renderBlocks.map((block, i) => (
                      <div
                        key={i}
                        className="rounded-lg overflow-hidden"
                        style={{
                          border: "1px solid rgba(0, 240, 255, 0.08)",
                          background: "rgba(6, 8, 13, 0.5)",
                          minHeight: 200,
                          maxHeight: 400,
                        }}
                      >
                        <RenderBlockView block={block} />
                      </div>
                    ))}
                  </div>
                )}

                <div className="text-[10px] mt-1.5 opacity-40 font-mono">
                  {new Date(msg.timestamp).toLocaleTimeString([], {
                    hour: "2-digit",
                    minute: "2-digit",
                  })}
                </div>
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}
