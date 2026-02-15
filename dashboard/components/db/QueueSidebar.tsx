"use client";

import { useEffect, useState } from "react";
import { fetchQueueConnections, type QueueConnectionSafe } from "@/lib/api";

export default function QueueSidebar({
  refreshKey = 0,
  selectedId,
  onSelect,
}: {
  refreshKey?: number;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
}) {
  const [queues, setQueues] = useState<QueueConnectionSafe[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchQueueConnections()
      .then((qs) => {
        setQueues(qs);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [refreshKey]);

  return (
    <div
      className="h-full flex flex-col overflow-hidden"
      style={{
        background: "linear-gradient(180deg, #0c1018 0%, #0a0e15 100%)",
        borderRight: "1px solid rgba(255, 138, 0, 0.08)",
      }}
    >
      {/* Header */}
      <div className="px-4 py-3 shrink-0" style={{ borderBottom: "1px solid rgba(255, 138, 0, 0.06)" }}>
        <div className="text-[10px] text-slate-500 uppercase tracking-[0.15em] font-bold">
          Queue Connections
        </div>
        <div className="text-[9px] text-slate-600 font-mono mt-0.5">
          {queues.length} configured
        </div>
      </div>

      {/* List */}
      <div className="flex-1 overflow-y-auto py-2">
        {loading && (
          <div className="px-4 py-8 text-center">
            <span className="text-slate-600 text-[10px] font-mono animate-pulse">Loading...</span>
          </div>
        )}

        {error && (
          <div className="px-4 py-3">
            <span className="text-[10px] text-red-400 font-mono">{error}</span>
          </div>
        )}

        {!loading && queues.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">No queues yet</span>
          </div>
        )}

        {/* All Queues aggregate option */}
        {queues.length > 1 && onSelect && (
          <button
            onClick={() => onSelect(null)}
            className="w-full flex items-center gap-2 px-4 py-2 text-left transition-all hover:bg-white/[0.02]"
            style={{
              background: selectedId === null ? "rgba(255, 138, 0, 0.05)" : "transparent",
              borderLeft: selectedId === null ? "2px solid #ff8a00" : "2px solid transparent",
            }}
          >
            <span className="text-[10px] text-slate-400 font-mono font-bold">All Queues</span>
          </button>
        )}

        {queues.map((q) => (
          <button
            key={q.id}
            onClick={() => onSelect?.(q.id)}
            className="w-full flex items-center gap-2 px-4 py-2 text-left transition-all hover:bg-white/[0.02]"
            style={{
              background: selectedId === q.id ? "rgba(255, 138, 0, 0.05)" : "transparent",
              borderLeft: selectedId === q.id ? `2px solid ${q.color}` : "2px solid transparent",
            }}
          >
            {/* Status dot */}
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: q.enabled ? "#06d6a0" : "#64748b" }}
            />

            {/* Queue icon */}
            <svg
              width="12" height="12" viewBox="0 0 24 24" fill="none"
              stroke={q.color} strokeWidth="2" className="shrink-0"
            >
              <rect x="2" y="6" width="20" height="12" rx="2" />
              <path d="M12 6V4" />
              <path d="M12 20v-2" />
              <path d="M6 12h12" />
            </svg>

            <div className="min-w-0 flex-1">
              <span
                className="text-xs font-mono font-bold truncate block"
                style={{ color: q.color }}
              >
                {q.name}
              </span>
              <span className="text-[9px] text-slate-600 font-mono truncate block">
                {q.provider} &middot; {q.region}
              </span>
            </div>

            <span
              className="text-[9px] font-mono shrink-0"
              style={{ color: q.enabled ? "#06d6a0" : "#64748b" }}
            >
              {q.enabled ? "ON" : "OFF"}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
