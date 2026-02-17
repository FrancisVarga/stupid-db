"use client";

import { useEffect, useState, useCallback } from "react";
import { useParams } from "next/navigation";
import Link from "next/link";
import AthenaQueryPanel from "@/components/db/AthenaQueryPanel";
import AthenaQueryLogPanel from "@/components/db/AthenaQueryLog";
import {
  getAthenaSchema,
  refreshAthenaSchema,
  listAthenaConnections,
  type AthenaSchema,
  type AthenaConnectionSafe,
  type AthenaDatabase,
  type AthenaTable,
} from "@/lib/db/athena-connections";

export default function AthenaConnectionDetailPage() {
  const params = useParams();
  const id = params.id as string;

  const [connection, setConnection] = useState<AthenaConnectionSafe | null>(null);
  const [schema, setSchema] = useState<AthenaSchema | null>(null);
  const [schemaStatus, setSchemaStatus] = useState<string>("pending");
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);
  const [refreshing, setRefreshing] = useState(false);

  // Schema tree expand state
  const [expandedDbs, setExpandedDbs] = useState<Set<string>>(new Set());
  const [expandedTables, setExpandedTables] = useState<Set<string>>(new Set());

  // Right panel tab
  const [rightTab, setRightTab] = useState<"query" | "logs">("query");
  const [queryLogRefreshKey, setQueryLogRefreshKey] = useState(0);

  // Clipboard feedback
  const [copiedTable, setCopiedTable] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    Promise.all([
      listAthenaConnections().then((conns) => conns.find((c) => c.id === id)),
      getAthenaSchema(id),
    ])
      .then(([conn, schemaRes]) => {
        setConnection(conn || null);
        setSchema(schemaRes.schema);
        setSchemaStatus(schemaRes.schema_status);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [id, refreshKey]);

  const handleRefreshSchema = useCallback(async () => {
    setRefreshing(true);
    try {
      await refreshAthenaSchema(id);
      setRefreshKey((k) => k + 1);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setRefreshing(false);
    }
  }, [id]);

  const toggleDb = (name: string) => {
    setExpandedDbs((prev) => {
      const next = new Set(prev);
      next.has(name) ? next.delete(name) : next.add(name);
      return next;
    });
  };

  const toggleTable = (key: string) => {
    setExpandedTables((prev) => {
      const next = new Set(prev);
      next.has(key) ? next.delete(key) : next.add(key);
      return next;
    });
  };

  const handleCopyQuery = (dbName: string, tableName: string) => {
    const sql = `SELECT * FROM ${dbName}.${tableName} LIMIT 100`;
    navigator.clipboard.writeText(sql).then(() => {
      const key = `${dbName}.${tableName}`;
      setCopiedTable(key);
      setTimeout(() => setCopiedTable(null), 2000);
    });
  };

  if (loading) {
    return (
      <div className="h-screen flex items-center justify-center">
        <span className="text-slate-600 text-sm font-mono animate-pulse">
          Loading connection...
        </span>
      </div>
    );
  }

  if (!connection) {
    return (
      <div className="h-screen flex flex-col items-center justify-center gap-4">
        <span className="text-slate-500 text-sm font-mono">
          Connection not found
        </span>
        <Link
          href="/athena"
          className="text-sm font-mono transition-colors"
          style={{ color: "#10b981" }}
        >
          Back to Athena Manager
        </Link>
      </div>
    );
  }

  return (
    <div className="h-screen flex flex-col">
      {/* ── Header ──────────────────────────────────────────────────── */}
      <header
        className="px-6 py-3 shrink-0"
        style={{
          borderBottom: "1px solid rgba(16, 185, 129, 0.08)",
          background:
            "linear-gradient(180deg, rgba(16, 185, 129, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center justify-between">
          <div className="flex items-center gap-4">
            <Link
              href="/athena"
              className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
            >
              &larr; Athena Manager
            </Link>
            <div
              className="w-[1px] h-4"
              style={{ background: "rgba(16, 185, 129, 0.12)" }}
            />
            <h1
              className="text-lg font-bold tracking-wider font-mono"
              style={{ color: connection.color }}
            >
              {connection.name}
            </h1>
          </div>

          <div className="flex items-center gap-3">
            {/* Status badges */}
            <span
              className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
              style={{
                background: connection.enabled
                  ? "rgba(6, 214, 160, 0.1)"
                  : "rgba(100, 116, 139, 0.1)",
                color: connection.enabled ? "#06d6a0" : "#64748b",
              }}
            >
              {connection.enabled ? "enabled" : "disabled"}
            </span>
            <SchemaStatusBadge status={schemaStatus} />

            {/* Refresh Schema button */}
            <button
              onClick={handleRefreshSchema}
              disabled={refreshing}
              className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
              style={{
                background: "rgba(16, 185, 129, 0.1)",
                border: "1px solid rgba(16, 185, 129, 0.3)",
                color: "#10b981",
              }}
            >
              {refreshing ? (
                <span className="flex items-center gap-1.5">
                  <span className="inline-block w-2.5 h-2.5 border border-current border-t-transparent rounded-full animate-spin" />
                  Refreshing...
                </span>
              ) : (
                "Refresh Schema"
              )}
            </button>
          </div>
        </div>

        {/* Info line */}
        <div className="text-[10px] text-slate-600 font-mono mt-1 pl-[1px]">
          {connection.region} &middot; {connection.catalog} &middot;{" "}
          {connection.workgroup}
        </div>
      </header>

      {/* ── Error banner ────────────────────────────────────────────── */}
      {error && (
        <div
          className="mx-6 mt-3 flex items-center gap-3 px-4 py-2.5 rounded-lg"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span
            className="w-2 h-2 rounded-full shrink-0 animate-pulse"
            style={{ background: "#ff4757" }}
          />
          <span className="text-xs text-red-400 font-medium">{error}</span>
          <button
            onClick={() => setError(null)}
            className="ml-auto text-slate-600 hover:text-slate-400 text-xs"
          >
            Dismiss
          </button>
        </div>
      )}

      {/* ── Two-column body ─────────────────────────────────────────── */}
      <div className="flex-1 flex min-h-0">
        {/* Left: Schema tree */}
        <div
          className="shrink-0 overflow-y-auto"
          style={{
            width: 300,
            borderRight: "1px solid rgba(16, 185, 129, 0.06)",
          }}
        >
          <div className="p-4">
            <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
              Schema Browser
            </h2>

            {schemaStatus === "pending" && (
              <SchemaPlaceholder
                message="Schema discovery pending..."
                color="#fbbf24"
              />
            )}

            {schemaStatus === "fetching" && (
              <SchemaPlaceholder
                message="Fetching schema from Athena..."
                color="#3b82f6"
              />
            )}

            {schemaStatus === "error" && (
              <div className="flex flex-col items-center gap-3 py-8">
                <span className="text-xs text-red-400 font-mono text-center">
                  Schema fetch failed
                </span>
                <button
                  onClick={handleRefreshSchema}
                  disabled={refreshing}
                  className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
                  style={{
                    background: "rgba(255, 71, 87, 0.1)",
                    border: "1px solid rgba(255, 71, 87, 0.3)",
                    color: "#ff4757",
                  }}
                >
                  Retry
                </button>
              </div>
            )}

            {schemaStatus === "ready" && schema && (
              <SchemaTree
                databases={schema.databases}
                expandedDbs={expandedDbs}
                expandedTables={expandedTables}
                copiedTable={copiedTable}
                onToggleDb={toggleDb}
                onToggleTable={toggleTable}
                onCopyQuery={handleCopyQuery}
              />
            )}

            {schemaStatus === "ready" && !schema && (
              <div className="py-8 text-center">
                <span className="text-xs text-slate-600 font-mono">
                  No schema data available
                </span>
              </div>
            )}

            {schema?.fetched_at && (
              <div className="mt-4 pt-3" style={{ borderTop: "1px solid rgba(30, 41, 59, 0.3)" }}>
                <span className="text-[9px] text-slate-700 font-mono">
                  Fetched {new Date(schema.fetched_at).toLocaleString()}
                </span>
              </div>
            )}
          </div>
        </div>

        {/* Right: Query / Logs panel */}
        <div className="flex-1 min-w-0 flex flex-col">
          {/* Right panel tab bar */}
          <div
            className="flex items-center gap-1 px-4 pt-3 pb-1 shrink-0"
            style={{ borderBottom: "1px solid rgba(16, 185, 129, 0.06)" }}
          >
            <RightTabButton active={rightTab === "query"} onClick={() => setRightTab("query")}>
              Query
            </RightTabButton>
            <RightTabButton active={rightTab === "logs"} onClick={() => {
              setRightTab("logs");
              setQueryLogRefreshKey((k) => k + 1);
            }}>
              Costs &amp; Logs
            </RightTabButton>
          </div>

          {rightTab === "query" ? (
            <AthenaQueryPanel
              connectionId={id}
              defaultDatabase={connection.database}
            />
          ) : (
            <AthenaQueryLogPanel
              connectionId={id}
              refreshKey={queryLogRefreshKey}
            />
          )}
        </div>
      </div>
    </div>
  );
}

// ── Right panel tab button ─────────────────────────────────────────────

function RightTabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      onClick={onClick}
      className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all"
      style={{
        background: active ? "rgba(16, 185, 129, 0.1)" : "transparent",
        border: active
          ? "1px solid rgba(16, 185, 129, 0.3)"
          : "1px solid transparent",
        color: active ? "#10b981" : "#64748b",
      }}
    >
      {children}
    </button>
  );
}

// ── Schema tree component ──────────────────────────────────────────────

function SchemaTree({
  databases,
  expandedDbs,
  expandedTables,
  copiedTable,
  onToggleDb,
  onToggleTable,
  onCopyQuery,
}: {
  databases: AthenaDatabase[];
  expandedDbs: Set<string>;
  expandedTables: Set<string>;
  copiedTable: string | null;
  onToggleDb: (name: string) => void;
  onToggleTable: (key: string) => void;
  onCopyQuery: (dbName: string, tableName: string) => void;
}) {
  if (databases.length === 0) {
    return (
      <div className="py-8 text-center">
        <span className="text-xs text-slate-600 font-mono">
          No databases found
        </span>
      </div>
    );
  }

  return (
    <div className="space-y-0.5">
      {databases.map((db) => {
        const isDbExpanded = expandedDbs.has(db.name);
        return (
          <div key={db.name}>
            {/* Database row */}
            <button
              onClick={() => onToggleDb(db.name)}
              className="w-full flex items-center gap-2 px-2 py-1.5 rounded-md text-left transition-colors hover:bg-white/[0.02]"
            >
              <ChevronIcon expanded={isDbExpanded} />
              <FolderIcon />
              <span className="text-xs font-mono font-bold text-slate-300 truncate">
                {db.name}
              </span>
              <span className="ml-auto text-[9px] text-slate-700 font-mono shrink-0">
                {db.tables.length}
              </span>
            </button>

            {/* Tables */}
            {isDbExpanded && (
              <div className="ml-3">
                {db.tables.map((table) => {
                  const tableKey = `${db.name}.${table.name}`;
                  const isTableExpanded = expandedTables.has(tableKey);
                  return (
                    <div key={tableKey}>
                      {/* Table row */}
                      <div className="flex items-center group">
                        <button
                          onClick={() => onToggleTable(tableKey)}
                          className="flex-1 flex items-center gap-2 pl-3 pr-1 py-1 rounded-md text-left transition-colors hover:bg-white/[0.02]"
                        >
                          <ChevronIcon expanded={isTableExpanded} />
                          <GridIcon />
                          <span className="text-[11px] font-mono text-slate-400 truncate">
                            {table.name}
                          </span>
                          <span className="ml-auto text-[9px] text-slate-700 font-mono shrink-0">
                            {table.columns.length}
                          </span>
                        </button>
                        {/* Quick Query button */}
                        <button
                          onClick={() => onCopyQuery(db.name, table.name)}
                          className="opacity-0 group-hover:opacity-100 shrink-0 ml-1 mr-1 px-1.5 py-0.5 rounded text-[8px] font-mono font-bold uppercase tracking-wider transition-all hover:opacity-80"
                          style={{
                            background:
                              copiedTable === tableKey
                                ? "rgba(6, 214, 160, 0.15)"
                                : "rgba(16, 185, 129, 0.08)",
                            color:
                              copiedTable === tableKey ? "#06d6a0" : "#10b981",
                          }}
                          title={`Copy: SELECT * FROM ${db.name}.${table.name} LIMIT 100`}
                        >
                          {copiedTable === tableKey ? "Copied" : "SQL"}
                        </button>
                      </div>

                      {/* Columns */}
                      {isTableExpanded && (
                        <div className="ml-8 py-0.5">
                          {table.columns.map((col, colIdx) => (
                            <div
                              key={`${tableKey}.${colIdx}.${col.name}`}
                              className="flex items-center gap-2 px-2 py-0.5"
                            >
                              <span className="w-1 h-1 rounded-full shrink-0" style={{ background: "#334155" }} />
                              <span className="text-[10px] font-mono text-slate-500 truncate">
                                {col.name}
                              </span>
                              <span className="text-[9px] font-mono text-slate-700 shrink-0">
                                {col.data_type}
                              </span>
                            </div>
                          ))}
                          {table.columns.length === 0 && (
                            <span className="text-[9px] text-slate-700 font-mono pl-2">
                              No columns
                            </span>
                          )}
                        </div>
                      )}
                    </div>
                  );
                })}
                {db.tables.length === 0 && (
                  <span className="text-[9px] text-slate-700 font-mono pl-6 py-1 block">
                    No tables
                  </span>
                )}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

// ── Schema loading placeholder ─────────────────────────────────────────

function SchemaPlaceholder({
  message,
  color,
}: {
  message: string;
  color: string;
}) {
  return (
    <div className="flex flex-col items-center gap-3 py-8">
      <span
        className="inline-block w-5 h-5 border-2 border-t-transparent rounded-full animate-spin"
        style={{ borderColor: `${color}40`, borderTopColor: "transparent" }}
      />
      <span className="text-xs font-mono" style={{ color }}>
        {message}
      </span>
    </div>
  );
}

// ── Schema status badge (reused from parent page) ──────────────────────

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

// ── SVG icons ──────────────────────────────────────────────────────────

function ChevronIcon({ expanded }: { expanded: boolean }) {
  return (
    <svg
      width="10"
      height="10"
      viewBox="0 0 10 10"
      fill="none"
      className={`shrink-0 transition-transform ${expanded ? "rotate-90" : ""}`}
    >
      <path
        d="M3.5 2L6.5 5L3.5 8"
        stroke="#475569"
        strokeWidth="1.5"
        strokeLinecap="round"
        strokeLinejoin="round"
      />
    </svg>
  );
}

function FolderIcon() {
  return (
    <svg
      width="14"
      height="14"
      viewBox="0 0 24 24"
      fill="none"
      stroke="#10b981"
      strokeWidth="1.5"
      className="shrink-0"
    >
      <path d="M22 19a2 2 0 01-2 2H4a2 2 0 01-2-2V5a2 2 0 012-2h5l2 3h9a2 2 0 012 2z" />
    </svg>
  );
}

function GridIcon() {
  return (
    <svg
      width="12"
      height="12"
      viewBox="0 0 24 24"
      fill="none"
      stroke="#475569"
      strokeWidth="1.5"
      className="shrink-0"
    >
      <rect x="3" y="3" width="7" height="7" />
      <rect x="14" y="3" width="7" height="7" />
      <rect x="3" y="14" width="7" height="7" />
      <rect x="14" y="14" width="7" height="7" />
    </svg>
  );
}
