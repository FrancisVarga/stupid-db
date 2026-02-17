"use client";

import { useState, useCallback } from "react";
import { executeQuery, type QueryResult } from "@/lib/api-db";
import CodeEditor from "./CodeEditor";

interface QueryPanelProps {
  db: string;
}

export default function QueryPanel({ db }: QueryPanelProps) {
  const [sql, setSql] = useState("");
  const [result, setResult] = useState<QueryResult | null>(null);
  const [executing, setExecuting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const handleExecute = useCallback(async () => {
    if (!sql.trim()) return;
    setExecuting(true);
    setError(null);
    setResult(null);
    try {
      const res = await executeQuery(db, sql.trim());
      setResult(res);
    } catch (err) {
      setError((err as Error).message);
    } finally {
      setExecuting(false);
    }
  }, [db, sql]);

  return (
    <div className="flex flex-col h-full">
      {/* SQL input */}
      <div className="shrink-0 p-4">
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
            <button
              onClick={handleExecute}
              disabled={executing || !sql.trim()}
              className="px-4 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90 disabled:opacity-40"
              style={{
                color: "#06080d",
                background: executing ? "#475569" : "#00f0ff",
              }}
            >
              {executing ? "Executing..." : "Execute"}
            </button>
          </div>
        </div>
      </div>

      {/* Error */}
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

      {/* Results */}
      {result && (
        <div className="flex-1 flex flex-col min-h-0 mx-4 mb-4">
          {/* Result header */}
          <div
            className="flex items-center justify-between px-3 py-2 shrink-0 rounded-t-lg"
            style={{
              background: "rgba(6, 8, 13, 0.4)",
              borderBottom: "1px solid rgba(0, 240, 255, 0.06)",
            }}
          >
            <span className="text-[10px] text-slate-500 font-mono">
              {result.row_count.toLocaleString()} row{result.row_count !== 1 ? "s" : ""}
            </span>
            <span
              className="text-[10px] font-mono font-bold"
              style={{ color: "#06d6a0" }}
            >
              {result.duration_ms.toFixed(1)}ms
            </span>
          </div>

          {/* Result table */}
          <div
            className="flex-1 overflow-auto rounded-b-lg"
            style={{
              background: "rgba(6, 8, 13, 0.3)",
              border: "1px solid rgba(30, 41, 59, 0.4)",
              borderTop: "none",
            }}
          >
            {result.columns.length > 0 && (
              <table className="w-full text-[10px] font-mono">
                <thead className="sticky top-0" style={{ background: "#0c1018" }}>
                  <tr style={{ borderBottom: "1px solid #1e293b" }}>
                    {result.columns.map((col) => (
                      <th
                        key={col}
                        className="px-2 py-1.5 text-left text-slate-500 font-bold tracking-wider uppercase whitespace-nowrap"
                      >
                        {col}
                      </th>
                    ))}
                  </tr>
                </thead>
                <tbody>
                  {result.rows.map((row, i) => (
                    <tr
                      key={i}
                      className="transition-colors"
                      style={{ borderBottom: "1px solid #0f172a" }}
                      onMouseEnter={(e) =>
                        ((e.currentTarget as HTMLElement).style.background =
                          "rgba(0, 240, 255, 0.02)")
                      }
                      onMouseLeave={(e) =>
                        ((e.currentTarget as HTMLElement).style.background =
                          "transparent")
                      }
                    >
                      {result.columns.map((col) => (
                        <td
                          key={col}
                          className="px-2 py-1.5 text-slate-300 whitespace-nowrap"
                        >
                          {formatValue(row[col])}
                        </td>
                      ))}
                    </tr>
                  ))}
                </tbody>
              </table>
            )}
          </div>
        </div>
      )}

      {/* Empty state */}
      {!result && !error && !executing && (
        <div className="flex-1 flex items-center justify-center">
          <span className="text-slate-600 text-sm font-mono">
            Write a SQL query and press Execute
          </span>
        </div>
      )}
    </div>
  );
}

function formatValue(val: unknown): string {
  if (val === null || val === undefined) return "NULL";
  if (typeof val === "object") return JSON.stringify(val);
  return String(val);
}
