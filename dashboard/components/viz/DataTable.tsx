"use client";

import { useState, useMemo } from "react";
import { exportCSV } from "@/lib/export";

interface Props {
  data: Record<string, unknown>[];
  columns?: string[];
  title?: string;
  sortable?: boolean;
  limit?: number;
  onRowClick?: (row: Record<string, unknown>) => void;
}

export default function DataTable({
  data,
  columns: columnsProp,
  title,
  sortable = true,
  limit = 100,
  onRowClick,
}: Props) {
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortAsc, setSortAsc] = useState(true);
  const [filter, setFilter] = useState("");

  const columns = useMemo(() => {
    if (columnsProp && columnsProp.length > 0) return columnsProp;
    if (data.length === 0) return [];
    return Object.keys(data[0]);
  }, [columnsProp, data]);

  const filtered = useMemo(() => {
    if (!filter) return data;
    const lower = filter.toLowerCase();
    return data.filter((row) =>
      columns.some((col) =>
        String(row[col] ?? "")
          .toLowerCase()
          .includes(lower)
      )
    );
  }, [data, filter, columns]);

  const sorted = useMemo(() => {
    if (!sortCol) return filtered;
    return [...filtered].sort((a, b) => {
      const va = a[sortCol];
      const vb = b[sortCol];
      if (typeof va === "number" && typeof vb === "number") {
        return sortAsc ? va - vb : vb - va;
      }
      const sa = String(va ?? "");
      const sb = String(vb ?? "");
      return sortAsc ? sa.localeCompare(sb) : sb.localeCompare(sa);
    });
  }, [filtered, sortCol, sortAsc]);

  const rows = sorted.slice(0, limit);

  const handleSort = (col: string) => {
    if (!sortable) return;
    if (sortCol === col) {
      setSortAsc(!sortAsc);
    } else {
      setSortCol(col);
      setSortAsc(true);
    }
  };

  return (
    <div className="w-full h-full flex flex-col overflow-hidden">
      {/* Header */}
      <div
        className="flex items-center justify-between px-3 py-2 shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <div className="flex items-center gap-3">
          {title && (
            <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold">
              {title}
            </span>
          )}
          <span className="text-[10px] text-slate-600 font-mono">
            {filtered.length} rows
          </span>
        </div>
        <div className="flex items-center gap-2">
          <input
            type="text"
            value={filter}
            onChange={(e) => setFilter(e.target.value)}
            placeholder="Filter..."
            className="bg-transparent text-[10px] text-slate-400 placeholder-slate-700 outline-none border border-slate-800 rounded px-2 py-1 w-28 focus:border-cyan-900"
          />
          <button
            onClick={() => exportCSV(data as Record<string, string | number>[], title || "export")}
            className="text-[9px] font-bold tracking-wider uppercase px-2 py-1 rounded transition-all"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
            }}
          >
            CSV
          </button>
        </div>
      </div>

      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-[10px] font-mono">
          <thead>
            <tr style={{ borderBottom: "1px solid #1e293b" }}>
              {columns.map((col) => (
                <th
                  key={col}
                  onClick={() => handleSort(col)}
                  className="px-2 py-1.5 text-left text-slate-500 font-bold tracking-wider uppercase whitespace-nowrap"
                  style={{
                    cursor: sortable ? "pointer" : "default",
                    color: sortCol === col ? "#00f0ff" : undefined,
                  }}
                >
                  {col}
                  {sortCol === col && (
                    <span className="ml-1">{sortAsc ? "\u25B2" : "\u25BC"}</span>
                  )}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {rows.map((row, i) => (
              <tr
                key={i}
                onClick={() => onRowClick?.(row)}
                className="transition-colors"
                style={{
                  borderBottom: "1px solid #0f172a",
                  cursor: onRowClick ? "pointer" : "default",
                }}
                onMouseEnter={(e) =>
                  ((e.currentTarget as HTMLElement).style.background =
                    "rgba(0, 240, 255, 0.03)")
                }
                onMouseLeave={(e) =>
                  ((e.currentTarget as HTMLElement).style.background =
                    "transparent")
                }
              >
                {columns.map((col) => (
                  <td
                    key={col}
                    className="px-2 py-1.5 text-slate-300 whitespace-nowrap"
                  >
                    {formatCellValue(row[col])}
                  </td>
                ))}
              </tr>
            ))}
          </tbody>
        </table>
        {sorted.length > limit && (
          <div className="text-center py-2 text-[10px] text-slate-600">
            Showing {limit} of {sorted.length} rows
          </div>
        )}
      </div>
    </div>
  );
}

function formatCellValue(val: unknown): string {
  if (val === null || val === undefined) return "-";
  if (typeof val === "number") {
    if (Number.isInteger(val)) return val.toLocaleString();
    return val.toFixed(4);
  }
  if (typeof val === "boolean") return val ? "true" : "false";
  return String(val);
}
