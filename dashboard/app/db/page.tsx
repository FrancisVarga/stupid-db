"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import DatabaseSidebar from "@/components/db/DatabaseSidebar";
import ConnectionForm from "@/components/db/ConnectionForm";
import { fetchDatabases, deleteConnectionApi, type Database, type ConnectionSafe } from "@/lib/api-db";

export default function DatabaseBrowserPage() {
  const [databases, setDatabases] = useState<Database[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showAddForm, setShowAddForm] = useState(false);
  const [editingConnection, setEditingConnection] = useState<ConnectionSafe | null>(null);
  const [sidebarKey, setSidebarKey] = useState(0);
  const [syncing, setSyncing] = useState(false);

  const loadDatabases = useCallback(() => {
    setLoading(true);
    setError(null);
    fetchDatabases()
      .then((dbs) => {
        setDatabases(dbs);
        setLoading(false);
        setSidebarKey((k) => k + 1);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  useEffect(() => {
    loadDatabases();
  }, [loadDatabases]);

  const handleDelete = async (id: string, name: string) => {
    if (!confirm(`Remove connection "${name}"? This only removes the saved connection, not the database itself.`)) return;
    try {
      await deleteConnectionApi(id);
      loadDatabases();
    } catch (e) {
      setError((e as Error).message);
    }
  };

  const handleSyncCatalog = async () => {
    setSyncing(true);
    try {
      await fetch("/api/v1/meta/catalog/sync", { method: "POST" });
    } catch {
      // Non-critical — catalog sync failure shouldn't alarm the user
    } finally {
      setSyncing(false);
    }
  };

  const totalTables = databases.reduce((sum, db) => sum + db.table_count, 0);
  const connectedCount = databases.filter((d) => d.status === "connected").length;

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background: "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div className="w-[1px] h-4" style={{ background: "rgba(0, 240, 255, 0.12)" }} />
          <h1 className="text-lg font-bold tracking-wider" style={{ color: "#00f0ff" }}>
            Database Manager
          </h1>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={handleSyncCatalog}
            disabled={syncing}
            className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
            style={{
              background: "rgba(168, 85, 247, 0.1)",
              border: "1px solid rgba(168, 85, 247, 0.3)",
              color: "#a855f7",
            }}
          >
            {syncing ? "Syncing..." : "Sync Catalog"}
          </button>
          <button
            onClick={() => setShowAddForm(true)}
            className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              background: "rgba(0, 240, 255, 0.1)",
              border: "1px solid rgba(0, 240, 255, 0.3)",
              color: "#00f0ff",
            }}
          >
            + Add Connection
          </button>
        </div>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 260 }} className="shrink-0">
          <DatabaseSidebar refreshKey={sidebarKey} />
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
              <ConnectionForm
                editing={editingConnection ?? undefined}
                onSaved={() => {
                  setShowAddForm(false);
                  setEditingConnection(null);
                  loadDatabases();
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

          {/* Empty state — no connections configured */}
          {!loading && databases.length === 0 && !showAddForm && (
            <div className="flex flex-col items-center justify-center py-20">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5" className="mb-4">
                <ellipse cx="12" cy="5" rx="9" ry="3" />
                <path d="M21 12c0 1.66-4.03 3-9 3s-9-1.34-9-3" />
                <path d="M3 5v14c0 1.66 4.03 3 9 3s9-1.34 9-3V5" />
              </svg>
              <p className="text-slate-500 text-sm font-mono mb-2">No database connections configured</p>
              <p className="text-slate-600 text-xs font-mono mb-4">Add a PostgreSQL connection to get started</p>
              <button
                onClick={() => setShowAddForm(true)}
                className="px-4 py-2 rounded-lg text-xs font-bold uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  background: "rgba(0, 240, 255, 0.1)",
                  border: "1px solid rgba(0, 240, 255, 0.3)",
                  color: "#00f0ff",
                }}
              >
                + Add Your First Connection
              </button>
            </div>
          )}

          {/* Stats + database list */}
          {!loading && databases.length > 0 && (
            <>
              <div className="grid grid-cols-3 gap-4 mb-8">
                <StatCard label="Connections" value={databases.length} accent="#00f0ff" />
                <StatCard label="Total Tables" value={totalTables} accent="#a855f7" />
                <StatCard
                  label="Status"
                  value={`${connectedCount}/${databases.length}`}
                  accent={connectedCount === databases.length ? "#06d6a0" : "#ff8a00"}
                />
              </div>

              <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-4">
                All Connections
              </h2>
              <div className="grid grid-cols-2 gap-3">
                {databases.map((db) => (
                  <div
                    key={db.id}
                    className="rounded-xl p-4 relative overflow-hidden group"
                    style={{
                      background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                      border: `1px solid ${db.status === "connected" ? `${db.color}20` : "rgba(255, 71, 87, 0.2)"}`,
                    }}
                  >
                    <div
                      className="absolute top-0 left-0 w-full h-[1px]"
                      style={{
                        background: `linear-gradient(90deg, transparent, ${db.color}60, transparent)`,
                      }}
                    />

                    {/* Status dot */}
                    <div className="flex items-center justify-between mb-2">
                      <div className="flex items-center gap-2">
                        <span
                          className="w-2 h-2 rounded-full shrink-0"
                          style={{
                            background: db.status === "connected" ? "#06d6a0" : "#ff4757",
                            boxShadow: db.status === "connected" ? "0 0 6px rgba(6, 214, 160, 0.5)" : "0 0 6px rgba(255, 71, 87, 0.5)",
                          }}
                        />
                        <Link
                          href={`/db/${encodeURIComponent(db.id)}`}
                          className="text-sm font-bold font-mono tracking-wide hover:opacity-80"
                          style={{ color: db.color }}
                        >
                          {db.name}
                        </Link>
                      </div>
                      <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-all">
                        <button
                          onClick={() => {
                            setShowAddForm(false);
                            setEditingConnection(db);
                          }}
                          className="text-slate-700 hover:text-purple-400 text-[10px]"
                          title="Edit connection"
                        >
                          Edit
                        </button>
                        <button
                          onClick={() => handleDelete(db.id, db.name)}
                          className="text-slate-700 hover:text-red-400 text-[10px]"
                          title="Remove connection"
                        >
                          ✕
                        </button>
                      </div>
                    </div>

                    <div className="text-[9px] text-slate-600 font-mono mb-1">
                      {db.host}:{db.port}/{db.database}
                    </div>

                    {db.status === "error" && (
                      <div className="text-[9px] text-red-400/70 font-mono truncate mb-1">
                        {db.error}
                      </div>
                    )}

                    <div className="flex items-center gap-3 mt-2">
                      <span className="text-[10px] text-slate-500 font-mono">
                        {db.table_count} table{db.table_count !== 1 ? "s" : ""}
                      </span>
                      <span className="text-[9px] text-slate-600 font-mono">{db.size}</span>
                      {db.status === "connected" && (
                        <a
                          href={`/api/v1/${encodeURIComponent(db.id)}/docs`}
                          target="_blank"
                          rel="noopener noreferrer"
                          className="text-[9px] font-bold uppercase tracking-wider hover:opacity-80"
                          style={{ color: "#a855f7" }}
                        >
                          API Docs
                        </a>
                      )}
                    </div>
                  </div>
                ))}
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
  accent = "#00f0ff",
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
