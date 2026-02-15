"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import AthenaSidebar from "@/components/db/AthenaSidebar";
import AthenaConnectionForm from "@/components/db/AthenaConnectionForm";
import {
  listAthenaConnections,
  deleteAthenaConnection,
  type AthenaConnectionSafe,
} from "@/lib/db/athena-connections";

export default function AthenaManagerPage() {
  const [connections, setConnections] = useState<AthenaConnectionSafe[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConnection, setEditingConnection] = useState<AthenaConnectionSafe | null>(null);
  const [sidebarKey, setSidebarKey] = useState(0);

  const loadConnections = useCallback(() => {
    setLoading(true);
    setError(null);
    listAthenaConnections()
      .then((conns) => {
        setConnections(conns);
        setLoading(false);
        setSidebarKey((k) => k + 1);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    loadConnections();
  }, [loadConnections]);

  const handleDelete = async (id: string, name: string) => {
    if (!confirm(`Remove connection "${name}"? This only removes the saved connection, not the Athena workgroup itself.`)) return;
    try {
      await deleteAthenaConnection(id);
      loadConnections();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const totalTables = connections.reduce((sum, conn) => {
    if (!conn.schema?.databases) return sum;
    return sum + conn.schema.databases.reduce(
      (dbSum, db) => dbSum + db.tables.length,
      0,
    );
  }, 0);

  const readyCount = connections.filter((c) => c.schema_status === "ready").length;

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(16, 185, 129, 0.08)",
          background: "linear-gradient(180deg, rgba(16, 185, 129, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div className="w-[1px] h-4" style={{ background: "rgba(16, 185, 129, 0.12)" }} />
          <h1 className="text-lg font-bold tracking-wider" style={{ color: "#10b981" }}>
            Athena Manager
          </h1>
        </div>
        <button
          onClick={() => setShowAddForm(true)}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            background: "rgba(16, 185, 129, 0.1)",
            border: "1px solid rgba(16, 185, 129, 0.3)",
            color: "#10b981",
          }}
        >
          + Add Connection
        </button>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 260 }} className="shrink-0">
          <AthenaSidebar refreshKey={sidebarKey} />
        </div>

        <div className="flex-1 overflow-y-auto px-8 py-6">
          {/* Error */}
          {error && (
            <div
              className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
              }}
            >
              <span className="w-2 h-2 rounded-full shrink-0 animate-pulse" style={{ background: "#ff4757" }} />
              <span className="text-xs text-red-400 font-medium">{error}</span>
            </div>
          )}

          {/* Add / Edit Connection Form */}
          {(showAddForm || editingConnection) && (
            <div className="mb-6">
              <AthenaConnectionForm
                editing={editingConnection ?? undefined}
                onSaved={() => {
                  setShowAddForm(false);
                  setEditingConnection(null);
                  loadConnections();
                }}
                onCancel={() => {
                  setShowAddForm(false);
                  setEditingConnection(null);
                }}
              />
            </div>
          )}

          {/* Loading */}
          {loading && (
            <div className="flex items-center justify-center py-20">
              <span className="text-slate-600 text-sm font-mono animate-pulse">Loading connections...</span>
            </div>
          )}

          {/* Empty state */}
          {!loading && connections.length === 0 && !showAddForm && (
            <div className="flex flex-col items-center justify-center py-20">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5" className="mb-4">
                <ellipse cx="12" cy="5" rx="9" ry="3" />
                <path d="M21 12c0 1.66-4.03 3-9 3s-9-1.34-9-3" />
                <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5" />
              </svg>
              <p className="text-slate-500 text-sm font-mono mb-2">No Athena connections configured</p>
              <p className="text-slate-600 text-xs font-mono mb-4">Add your first Athena connection</p>
              <button
                onClick={() => setShowAddForm(true)}
                className="px-4 py-2 rounded-lg text-xs font-bold uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  background: "rgba(16, 185, 129, 0.1)",
                  border: "1px solid rgba(16, 185, 129, 0.3)",
                  color: "#10b981",
                }}
              >
                + Add Your First Connection
              </button>
            </div>
          )}

          {/* Stats + connection list */}
          {!loading && connections.length > 0 && (
            <>
              <div className="grid grid-cols-3 gap-4 mb-8">
                <StatCard label="Connections" value={connections.length} accent="#10b981" />
                <StatCard label="Total Tables" value={totalTables} accent="#a855f7" />
                <StatCard
                  label="Schema Status"
                  value={`${readyCount}/${connections.length}`}
                  accent={readyCount === connections.length ? "#06d6a0" : "#10b981"}
                />
              </div>

              <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-4">
                All Connections
              </h2>
              <div className="grid grid-cols-2 gap-3">
                {connections.map((conn) => {
                  const connTableCount = conn.schema?.databases
                    ? conn.schema.databases.reduce((s, db) => s + db.tables.length, 0)
                    : 0;

                  return (
                    <div
                      key={conn.id}
                      className="rounded-xl p-4 relative overflow-hidden group"
                      style={{
                        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                        border: `1px solid ${conn.enabled ? `${conn.color}20` : "rgba(100, 116, 139, 0.2)"}`,
                      }}
                    >
                      <div
                        className="absolute top-0 left-0 w-full h-[1px]"
                        style={{
                          background: `linear-gradient(90deg, transparent, ${conn.color}60, transparent)`,
                        }}
                      />

                      {/* Status dot + name */}
                      <div className="flex items-center justify-between mb-2">
                        <div className="flex items-center gap-2">
                          <span
                            className="w-2 h-2 rounded-full shrink-0"
                            style={{
                              background: conn.enabled ? "#06d6a0" : "#64748b",
                              boxShadow: conn.enabled ? "0 0 6px rgba(6, 214, 160, 0.5)" : "none",
                            }}
                          />
                          <Link
                            href={`/athena/${encodeURIComponent(conn.id)}`}
                            className="text-sm font-bold font-mono tracking-wide hover:opacity-80"
                            style={{ color: conn.color }}
                          >
                            {conn.name}
                          </Link>
                        </div>
                        <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                          <button
                            onClick={() => {
                              setShowAddForm(false);
                              setEditingConnection(conn);
                            }}
                            className="text-slate-700 hover:text-purple-400 text-[10px]"
                            title="Edit connection"
                          >
                            Edit
                          </button>
                          <button
                            onClick={() => handleDelete(conn.id, conn.name)}
                            className="text-slate-700 hover:text-red-400 text-[10px]"
                            title="Remove connection"
                          >
                            ✕
                          </button>
                        </div>
                      </div>

                      {/* Region / catalog / workgroup */}
                      <div className="text-[9px] text-slate-600 font-mono mb-1">
                        {conn.region} &middot; {conn.catalog} &middot; {conn.workgroup}
                      </div>

                      {/* Schema status badge */}
                      <div className="flex items-center gap-3 mt-2">
                        <SchemaStatusBadge status={conn.schema_status} />
                        <span className="text-[10px] text-slate-500 font-mono">
                          {connTableCount} table{connTableCount !== 1 ? "s" : ""}
                        </span>
                      </div>
                    </div>
                  );
                })}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}

function StatCard({
  label,
  value,
  accent = "#10b981",
}: {
  label: string;
  value: string | number;
  accent?: string;
}) {
  return (
    <div className="stat-card rounded-xl p-4 relative overflow-hidden">
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-2xl font-bold font-mono mt-1" style={{ color: accent }}>
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}

function SchemaStatusBadge({ status }: { status: string }) {
  let bg: string;
  let color: string;
  let pulse = false;

  switch (status) {
    case "ready":
      bg = "rgba(6, 214, 160, 0.1)";
      color = "#06d6a0";
      break;
    case "pending":
      bg = "rgba(250, 204, 21, 0.1)";
      color = "#facc15";
      pulse = true;
      break;
    case "fetching":
      bg = "rgba(59, 130, 246, 0.1)";
      color = "#3b82f6";
      pulse = true;
      break;
    case "error":
      bg = "rgba(255, 71, 87, 0.1)";
      color = "#ff4757";
      break;
    default:
      bg = "rgba(100, 116, 139, 0.1)";
      color = "#64748b";
  }

  return (
    <span
      className={`text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded${pulse ? " animate-pulse" : ""}`}
      style={{ background: bg, color }}
    >
      {status}
    </span>
  );
}
