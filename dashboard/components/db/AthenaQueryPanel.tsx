"use client";

import { useState, useRef, useCallback, useEffect } from "react";
import { executeAthenaQuery, downloadAthenaParquet } from "@/lib/db/athena-query";
import CodeEditor from "./CodeEditor";

interface AthenaQueryPanelProps {
  connectionId: string;
  defaultDatabase?: string;
  /** When set externally (e.g. from AI chat), overrides the editor content. */
  externalSql?: string;
  /** Called when a query fails — used for AI error feedback loop. */
  onQueryError?: (error: string, sql: string) => void;
  /** Ref to trigger execution from outside. */
  executeRef?: React.RefObject<(() => void) | null>;
}

export default function AthenaQueryPanel({
  connectionId,
  defaultDatabase,
  externalSql,
  onQueryError,
  executeRef,
}: AthenaQueryPanelProps) {
  const [sql, setSql] = useState("");
  const [dbOverride, setDbOverride] = useState("");
  const [status, setStatus] = useState<string | null>(null);
  const [queryId, setQueryId] = useState<string | null>(null);
  const [columns, setColumns] = useState<string[]>([]);
  const [rows, setRows] = useState<string[][]>([]);
  const [error, setError] = useState<string | null>(null);
  const [executing, setExecuting] = useState(false);
  const [totalRows, setTotalRows] = useState<number | null>(null);
  const [elapsed, setElapsed] = useState<number>(0);

  const [exporting, setExporting] = useState(false);

  const controllerRef = useRef<AbortController | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);

  // Pick up externally-injected SQL (from AI chat)
  useEffect(() => {
    if (externalSql !== undefined && externalSql !== sql) {
      setSql(externalSql);
    }
    // Only react to externalSql changes, not sql
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [externalSql]);

  // Duration timer
  useEffect(() => {
    if (executing) {
      const start = Date.now();
      setElapsed(0);
      timerRef.current = setInterval(() => {
        setElapsed(Date.now() - start);
      }, 100);
    } else if (timerRef.current) {
      clearInterval(timerRef.current);
      timerRef.current = null;
    }
    return () => {
      if (timerRef.current) clearInterval(timerRef.current);
    };
  }, [executing]);

  const handleExecute = useCallback(() => {
    if (!sql.trim()) return;

    // Reset state
    setColumns([]);
    setRows([]);
    setError(null);
    setStatus(null);
    setQueryId(null);
    setTotalRows(null);
    setExecuting(true);

    controllerRef.current = executeAthenaQuery(
      connectionId,
      sql.trim(),
      dbOverride || defaultDatabase || undefined,
      {
        onStatus: (state, qid, message) => {
          setStatus(state);
          setQueryId(qid);
          if (message && state === "FAILED") {
            setError(message);
            setExecuting(false);
            onQueryError?.(message, sql.trim());
          }
        },
        onColumns: (cols) => setColumns(cols),
        onRows: (batch) => setRows((prev) => [...prev, ...batch]),
        onDone: (total, qid) => {
          setTotalRows(total);
          setQueryId(qid);
          setExecuting(false);
        },
        onError: (msg) => {
          setError(msg);
          setExecuting(false);
          onQueryError?.(msg, sql.trim());
        },
      },
    );
  }, [connectionId, sql, dbOverride, defaultDatabase]);

  const handleCancel = useCallback(() => {
    controllerRef.current?.abort();
    setExecuting(false);
    setStatus("CANCELLED");
  }, []);

  const handleExportParquet = useCallback(async () => {
    if (!sql.trim()) return;
    setExporting(true);
    try {
      await downloadAthenaParquet(
        connectionId,
        sql.trim(),
        dbOverride || defaultDatabase || undefined,
      );
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setExporting(false);
    }
  }, [connectionId, sql, dbOverride, defaultDatabase]);

  // Expose execute function to parent via ref
  useEffect(() => {
    if (executeRef) {
      (executeRef as React.MutableRefObject<(() => void) | null>).current = handleExecute;
    }
    return () => {
      if (executeRef) {
        (executeRef as React.MutableRefObject<(() => void) | null>).current = null;
      }
    };
  }, [executeRef, handleExecute]);

  const hasResults = columns.length > 0;
  const showEmpty = !hasResults && !error && !executing && !status;

  return (
    <div className="flex flex-col h-full">
      {/* ── Input area ──────────────────────────────────────────── */}
      <div className="shrink-0 p-4">
        {/* Database override */}
        <div className="mb-2">
          <input
            type="text"
            value={dbOverride}
            onChange={(e) => setDbOverride(e.target.value)}
            placeholder={
              defaultDatabase
                ? `Database (default: ${defaultDatabase})`
                : "Database override (optional)"
            }
            spellCheck={false}
            className="w-full bg-transparent text-xs text-slate-300 font-mono rounded-lg px-3 py-2 outline-none"
            style={{
              background: "rgba(6, 8, 13, 0.6)",
              border: "1px solid rgba(30, 41, 59, 0.6)",
            }}
          />
        </div>

        {/* SQL editor */}
        <div>
          <CodeEditor
            value={sql}
            onChange={setSql}
            language="sql"
            placeholder="SELECT * FROM ..."
            minHeight="100px"
            maxHeight="200px"
            onSubmit={handleExecute}
          />
          <div className="flex items-center justify-between mt-2">
            <span className="text-[9px] text-slate-600 font-mono">
              Ctrl+Enter to execute
            </span>
            <div className="flex items-center gap-2">
              {executing && (
                <button
                  onClick={handleCancel}
                  className="px-4 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90"
                  style={{
                    color: "#fbbf24",
                    background: "rgba(251, 191, 36, 0.1)",
                    border: "1px solid rgba(251, 191, 36, 0.3)",
                  }}
                >
                  Cancel
                </button>
              )}
              <button
                onClick={handleExecute}
                disabled={executing || !sql.trim()}
                className="px-4 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90 disabled:opacity-40"
                style={{
                  color: "#06080d",
                  background: executing ? "#475569" : "#10b981",
                }}
              >
                {executing ? "Executing..." : "Execute"}
              </button>
            </div>
          </div>
        </div>
      </div>

      {/* ── Status bar ──────────────────────────────────────────── */}
      {status && (
        <div
          className="mx-4 mb-3 flex items-center justify-between rounded-lg px-3 py-2"
          style={{
            background: "rgba(16, 185, 129, 0.04)",
            border: "1px solid rgba(16, 185, 129, 0.15)",
          }}
        >
          <div className="flex items-center gap-2">
            <StatusDot state={status} />
            <span className="text-[10px] text-slate-400 font-mono uppercase tracking-wider">
              {status}
            </span>
            {queryId && (
              <span className="text-[9px] text-slate-600 font-mono">
                {queryId}
              </span>
            )}
          </div>
          <span className="text-[10px] text-slate-500 font-mono">
            {formatDuration(elapsed)}
          </span>
        </div>
      )}

      {/* ── Error ───────────────────────────────────────────────── */}
      {error && (
        <div
          className="mx-4 mb-3 rounded-lg px-3 py-2 text-xs text-red-400 font-mono"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          {error}
        </div>
      )}

      {/* ── Results table ───────────────────────────────────────── */}
      {hasResults && (
        <div className="flex-1 flex flex-col min-h-0 mx-4 mb-4">
          {/* Result header */}
          <div
            className="flex items-center justify-between px-3 py-2 shrink-0 rounded-t-lg"
            style={{
              background: "rgba(6, 8, 13, 0.4)",
              borderBottom: "1px solid rgba(16, 185, 129, 0.06)",
            }}
          >
            <span className="text-[10px] text-slate-500 font-mono">
              {totalRows !== null
                ? `${totalRows.toLocaleString()} row${totalRows !== 1 ? "s" : ""}`
                : `${rows.length.toLocaleString()} row${rows.length !== 1 ? "s" : ""} (streaming...)`}
            </span>
            <div className="flex items-center gap-3">
              <button
                onClick={handleExportParquet}
                disabled={exporting || executing}
                className="px-3 py-1 text-[9px] font-bold tracking-wider uppercase rounded transition-all hover:opacity-90 disabled:opacity-40"
                style={{
                  color: "#a78bfa",
                  background: "rgba(167, 139, 250, 0.08)",
                  border: "1px solid rgba(167, 139, 250, 0.2)",
                }}
              >
                {exporting ? "Exporting..." : "Parquet"}
              </button>
              <span
                className="text-[10px] font-mono font-bold"
                style={{ color: "#10b981" }}
              >
                {formatDuration(elapsed)}
              </span>
            </div>
          </div>

          {/* Scrollable table */}
          <div
            className="flex-1 overflow-auto rounded-b-lg"
            style={{
              background: "rgba(6, 8, 13, 0.3)",
              border: "1px solid rgba(30, 41, 59, 0.4)",
              borderTop: "none",
            }}
          >
            <table className="w-full text-[10px] font-mono">
              <thead className="sticky top-0" style={{ background: "#0c1018" }}>
                <tr style={{ borderBottom: "1px solid #1e293b" }}>
                  {columns.map((col) => (
                    <th
                      key={col}
                      className="px-2 py-1.5 text-left font-bold tracking-wider uppercase whitespace-nowrap"
                      style={{ color: "#10b981" }}
                    >
                      {col}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {rows.map((row, i) => (
                  <tr
                    key={i}
                    className="transition-colors"
                    style={{ borderBottom: "1px solid #0f172a" }}
                    onMouseEnter={(e) =>
                      ((e.currentTarget as HTMLElement).style.background =
                        "rgba(16, 185, 129, 0.02)")
                    }
                    onMouseLeave={(e) =>
                      ((e.currentTarget as HTMLElement).style.background =
                        "transparent")
                    }
                  >
                    {row.map((cell, j) => (
                      <td
                        key={j}
                        className="px-2 py-1.5 text-slate-300 whitespace-nowrap"
                      >
                        {cell === null || cell === undefined ? (
                          <span className="text-slate-600 italic">NULL</span>
                        ) : (
                          String(cell)
                        )}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </div>
      )}

      {/* ── Empty state ─────────────────────────────────────────── */}
      {showEmpty && (
        <div className="flex-1 flex items-center justify-center">
          <span className="text-slate-600 text-sm font-mono">
            Write a SQL query and press Execute
          </span>
        </div>
      )}
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────

function StatusDot({ state }: { state: string }) {
  let color: string;
  let animate = false;

  switch (state) {
    case "QUEUED":
      color = "#fbbf24";
      animate = true;
      break;
    case "RUNNING":
      color = "#10b981";
      animate = true;
      break;
    case "SUCCEEDED":
      color = "#10b981";
      break;
    case "FAILED":
      color = "#ef4444";
      break;
    case "CANCELLED":
      color = "#94a3b8";
      break;
    default:
      color = "#64748b";
  }

  return (
    <span
      className={`inline-block w-1.5 h-1.5 rounded-full ${animate ? "animate-pulse" : ""}`}
      style={{ background: color }}
    />
  );
}

function formatDuration(ms: number): string {
  if (ms < 1000) return `${ms}ms`;
  const s = ms / 1000;
  if (s < 60) return `${s.toFixed(1)}s`;
  const m = Math.floor(s / 60);
  const rem = s % 60;
  return `${m}m ${rem.toFixed(0)}s`;
}
