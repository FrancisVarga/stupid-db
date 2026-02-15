"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import { usePathname } from "next/navigation";
import {
  fetchDatabases,
  fetchTables,
  type Database,
  type Table,
} from "@/lib/api-db";

export default function DatabaseSidebar({ refreshKey = 0 }: { refreshKey?: number }) {
  const pathname = usePathname();
  const [databases, setDatabases] = useState<Database[]>([]);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [tables, setTables] = useState<Record<string, Table[]>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  // Determine which connection/table is active from the URL
  const pathParts = pathname.split("/").filter(Boolean);
  // /db/conn-id/mytable => ["db", "conn-id", "mytable"]
  const activeDb = pathParts.length >= 2 && pathParts[0] === "db" ? pathParts[1] : null;
  const activeTable =
    pathParts.length >= 3 && pathParts[0] === "db" ? pathParts[2] : null;

  useEffect(() => {
    fetchDatabases()
      .then((dbs) => {
        setDatabases(dbs);
        setLoading(false);
        // Auto-expand the active connection
        if (activeDb) {
          setExpanded((prev) => ({ ...prev, [activeDb]: true }));
          fetchTables(activeDb)
            .then((t) => setTables((prev) => ({ ...prev, [activeDb]: t })))
            .catch(() => {});
        }
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [refreshKey]); // eslint-disable-line react-hooks/exhaustive-deps

  const toggleDb = useCallback(
    (connId: string) => {
      setExpanded((prev) => {
        const next = { ...prev, [connId]: !prev[connId] };
        if (next[connId] && !tables[connId]) {
          fetchTables(connId)
            .then((t) => setTables((p) => ({ ...p, [connId]: t })))
            .catch(() => {});
        }
        return next;
      });
    },
    [tables],
  );

  return (
    <div
      className="h-full flex flex-col overflow-hidden"
      style={{
        background: "linear-gradient(180deg, #0c1018 0%, #0a0e15 100%)",
        borderRight: "1px solid rgba(0, 240, 255, 0.08)",
      }}
    >
      {/* Header */}
      <div className="px-4 py-3 shrink-0" style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}>
        <div className="text-[10px] text-slate-500 uppercase tracking-[0.15em] font-bold">
          Connections
        </div>
        <div className="text-[9px] text-slate-600 font-mono mt-0.5">
          {databases.length} configured
        </div>
      </div>

      {/* Tree */}
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

        {!loading && databases.length === 0 && (
          <div className="px-4 py-8 text-center">
            <span className="text-[10px] text-slate-600 font-mono">No connections yet</span>
          </div>
        )}

        {databases.map((db) => {
          const isExpanded = expanded[db.id];
          const isActive = activeDb === db.id;
          const dbTables = tables[db.id] || [];

          return (
            <div key={db.id}>
              {/* Connection row */}
              <button
                onClick={() => toggleDb(db.id)}
                className="w-full flex items-center gap-2 px-4 py-2 text-left transition-all hover:bg-white/[0.02]"
                style={{ background: isActive ? "rgba(0, 240, 255, 0.03)" : "transparent" }}
              >
                {/* Expand arrow */}
                <svg
                  width="10" height="10" viewBox="0 0 10 10"
                  className="shrink-0 transition-transform"
                  style={{
                    transform: isExpanded ? "rotate(90deg)" : "rotate(0deg)",
                    fill: isActive ? db.color : "#475569",
                  }}
                >
                  <path d="M3 1l4 4-4 4z" />
                </svg>

                {/* Status dot */}
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: db.status === "connected" ? "#06d6a0" : "#ff4757" }}
                />

                {/* DB icon */}
                <svg
                  width="12" height="12" viewBox="0 0 24 24" fill="none"
                  stroke={isActive ? db.color : "#475569"} strokeWidth="2" className="shrink-0"
                >
                  <ellipse cx="12" cy="5" rx="9" ry="3" />
                  <path d="M21 12c0 1.66-4.03 3-9 3s-9-1.34-9-3" />
                  <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5" />
                </svg>

                <span
                  className="text-xs font-mono font-bold truncate"
                  style={{ color: isActive ? db.color : "#94a3b8" }}
                >
                  {db.name}
                </span>

                <span className="ml-auto text-[9px] text-slate-600 font-mono shrink-0">
                  {db.table_count}
                </span>
              </button>

              {/* Tables list */}
              {isExpanded && (
                <div className="ml-4">
                  {db.status === "error" && (
                    <div className="px-4 py-1.5 text-[9px] text-red-400/70 font-mono truncate">
                      {db.error}
                    </div>
                  )}

                  {db.status === "connected" && dbTables.length === 0 && (
                    <div className="px-4 py-1.5 text-[9px] text-slate-600 font-mono animate-pulse">
                      Loading tables...
                    </div>
                  )}

                  {dbTables.map((t) => {
                    const isTableActive = isActive && activeTable === t.name;
                    return (
                      <Link
                        key={t.name}
                        href={`/db/${encodeURIComponent(db.id)}/${encodeURIComponent(t.name)}`}
                        className="flex items-center gap-2 px-4 py-1.5 transition-all hover:bg-white/[0.02]"
                        style={{
                          background: isTableActive ? "rgba(0, 240, 255, 0.05)" : "transparent",
                          borderLeft: isTableActive ? `2px solid ${db.color}` : "2px solid transparent",
                        }}
                      >
                        <svg
                          width="10" height="10" viewBox="0 0 24 24" fill="none"
                          stroke={isTableActive ? db.color : "#334155"} strokeWidth="2" className="shrink-0"
                        >
                          <rect x="3" y="3" width="18" height="18" rx="2" />
                          <line x1="3" y1="9" x2="21" y2="9" />
                          <line x1="9" y1="3" x2="9" y2="21" />
                        </svg>
                        <span
                          className="text-[10px] font-mono truncate"
                          style={{ color: isTableActive ? db.color : "#64748b" }}
                        >
                          {t.name}
                        </span>
                        <span className="ml-auto text-[8px] text-slate-700 font-mono shrink-0">
                          ~{formatRowCount(t.estimated_rows)}
                        </span>
                      </Link>
                    );
                  })}

                  {db.status === "connected" && (
                    <a
                      href={`/api/v1/${encodeURIComponent(db.id)}/docs`}
                      target="_blank"
                      rel="noopener noreferrer"
                      className="flex items-center gap-2 px-4 py-1.5 mt-1 text-[9px] font-bold tracking-wider uppercase transition-all hover:opacity-80"
                      style={{ color: "#a855f7" }}
                    >
                      <svg
                        width="10" height="10" viewBox="0 0 24 24" fill="none"
                        stroke="#a855f7" strokeWidth="2" className="shrink-0"
                      >
                        <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" />
                        <polyline points="14 2 14 8 20 8" />
                      </svg>
                      API Docs
                    </a>
                  )}
                </div>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function formatRowCount(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}
