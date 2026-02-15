"use client";

import { useEffect, useState } from "react";
import { listAthenaConnections, type AthenaConnectionSafe } from "@/lib/db/athena-connections";

export default function AthenaSidebar({
  refreshKey = 0,
  selectedId,
  onSelect,
}: {
  refreshKey?: number;
  selectedId?: string | null;
  onSelect?: (id: string | null) => void;
}) {
  const [connections, setConnections] = useState<AthenaConnectionSafe[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    listAthenaConnections()
      .then((conns) => {
        setConnections(conns);
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
        borderRight: "1px solid rgba(16, 185, 129, 0.08)",
      }}
    >
      {/* Header */}
      <div className="px-4 py-3 shrink-0" style={{ borderBottom: "1px solid rgba(16, 185, 129, 0.06)" }}>
        <div className="text-[10px] text-slate-500 uppercase tracking-[0.15em] font-bold">
          Athena Connections
        </div>
        <div className="text-[9px] text-slate-600 font-mono mt-0.5">
          {connections.length} configured
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

        {!loading && connections.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">No connections yet</span>
          </div>
        )}

        {/* All Connections aggregate option */}
        {connections.length > 1 && onSelect && (
          <button
            onClick={() => onSelect(null)}
            className="w-full flex items-center gap-2 px-4 py-2 text-left transition-all hover:bg-white/[0.02]"
            style={{
              background: selectedId === null ? "rgba(16, 185, 129, 0.05)" : "transparent",
              borderLeft: selectedId === null ? "2px solid #10b981" : "2px solid transparent",
            }}
          >
            <span className="text-[10px] text-slate-400 font-mono font-bold">All Connections</span>
          </button>
        )}

        {connections.map((conn) => (
          <button
            key={conn.id}
            onClick={() => onSelect?.(conn.id)}
            className="w-full flex items-center gap-2 px-4 py-2 text-left transition-all hover:bg-white/[0.02]"
            style={{
              background: selectedId === conn.id ? "rgba(16, 185, 129, 0.05)" : "transparent",
              borderLeft: selectedId === conn.id ? `2px solid ${conn.color}` : "2px solid transparent",
            }}
          >
            {/* Status dot */}
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: conn.enabled ? "#06d6a0" : "#64748b" }}
            />

            {/* Athena icon — database cylinder */}
            <svg
              width="12" height="12" viewBox="0 0 24 24" fill="none"
              stroke={conn.color} strokeWidth="2" className="shrink-0"
            >
              <ellipse cx="12" cy="5" rx="9" ry="3" />
              <path d="M21 12c0 1.66-4.03 3-9 3s-9-1.34-9-3" />
              <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5" />
            </svg>

            <div className="min-w-0 flex-1">
              <span
                className="text-xs font-mono font-bold truncate block"
                style={{ color: conn.color }}
              >
                {conn.name}
              </span>
              <span className="text-[9px] text-slate-600 font-mono truncate block">
                {conn.region} &middot; {conn.workgroup}
              </span>
            </div>

            <span
              className="text-[9px] font-mono shrink-0"
              style={{ color: conn.enabled ? "#06d6a0" : "#64748b" }}
            >
              {conn.enabled ? "ON" : "OFF"}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}
