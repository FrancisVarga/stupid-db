"use client";

import { useState, useMemo, useCallback } from "react";
import type { Column } from "@/lib/api-db";
import TypeCell, { TypeBadge } from "./TypeCell";

interface DataGridProps {
  rows: Record<string, unknown>[];
  columns: Column[];
  total: number;
  page: number;
  limit: number;
  sortCol: string | null;
  sortOrder: "asc" | "desc";
  onSort: (col: string) => void;
  onPageChange: (page: number) => void;
  onLimitChange: (limit: number) => void;
  onRowClick: (row: Record<string, unknown>) => void;
  onSelectionChange: (ids: string[]) => void;
  pkColumn: string | null;
}

const PAGE_SIZES = [25, 50, 100];

export default function DataGrid({
  rows,
  columns,
  total,
  page,
  limit,
  sortCol,
  sortOrder,
  onSort,
  onPageChange,
  onLimitChange,
  onRowClick,
  onSelectionChange,
  pkColumn,
}: DataGridProps) {
  const [selected, setSelected] = useState<Set<string>>(new Set());

  const totalPages = Math.max(1, Math.ceil(total / limit));

  const columnNames = useMemo(() => columns.map((c) => c.name), [columns]);
  const columnMap = useMemo(() => {
    const map = new Map<string, Column>();
    for (const c of columns) map.set(c.name, c);
    return map;
  }, [columns]);

  const toggleAll = useCallback(() => {
    if (!pkColumn) return;
    if (selected.size === rows.length) {
      setSelected(new Set());
      onSelectionChange([]);
    } else {
      const allIds = rows.map((r) => String(r[pkColumn]));
      setSelected(new Set(allIds));
      onSelectionChange(allIds);
    }
  }, [rows, selected.size, pkColumn, onSelectionChange]);

  const toggleRow = useCallback(
    (id: string) => {
      setSelected((prev) => {
        const next = new Set(prev);
        if (next.has(id)) next.delete(id);
        else next.add(id);
        onSelectionChange(Array.from(next));
        return next;
      });
    },
    [onSelectionChange]
  );

  return (
    <div className="flex flex-col h-full">
      {/* Table */}
      <div className="flex-1 overflow-auto">
        <table className="w-full text-[10px] font-mono">
          <thead className="sticky top-0 z-10" style={{ background: "#0c1018" }}>
            <tr style={{ borderBottom: "1px solid #1e293b" }}>
              {/* Checkbox column */}
              {pkColumn && (
                <th className="px-2 py-2 w-8">
                  <input
                    type="checkbox"
                    checked={rows.length > 0 && selected.size === rows.length}
                    onChange={toggleAll}
                    className="accent-cyan-500"
                  />
                </th>
              )}
              {columnNames.map((name) => {
                const col = columnMap.get(name);
                const isActive = sortCol === name;
                return (
                  <th
                    key={name}
                    onClick={() => onSort(name)}
                    className="px-2 py-2 text-left whitespace-nowrap cursor-pointer select-none group"
                  >
                    <div className="flex items-center gap-1">
                      <span
                        className="text-[10px] font-bold tracking-wider uppercase transition-colors"
                        style={{ color: isActive ? "#00f0ff" : "#475569" }}
                      >
                        {name}
                      </span>
                      {col && <TypeBadge udtName={col.udt_name} />}
                      {col?.is_pk && (
                        <span
                          className="text-[7px] font-bold px-1 py-0.5 rounded"
                          style={{
                            color: "#ffe600",
                            background: "rgba(255, 230, 0, 0.1)",
                          }}
                        >
                          PK
                        </span>
                      )}
                      {isActive && (
                        <span className="text-[10px]" style={{ color: "#00f0ff" }}>
                          {sortOrder === "asc" ? "\u25B2" : "\u25BC"}
                        </span>
                      )}
                    </div>
                  </th>
                );
              })}
            </tr>
          </thead>
          <tbody>
            {rows.length === 0 ? (
              <tr>
                <td
                  colSpan={columnNames.length + (pkColumn ? 1 : 0)}
                  className="text-center py-12 text-slate-600 text-sm"
                >
                  No rows found
                </td>
              </tr>
            ) : (
              rows.map((row, i) => {
                const rowId = pkColumn ? String(row[pkColumn]) : String(i);
                const isSelected = selected.has(rowId);
                return (
                  <tr
                    key={rowId}
                    className="transition-colors"
                    style={{
                      borderBottom: "1px solid #0f172a",
                      background: isSelected
                        ? "rgba(0, 240, 255, 0.04)"
                        : "transparent",
                    }}
                    onMouseEnter={(e) => {
                      if (!isSelected)
                        (e.currentTarget as HTMLElement).style.background =
                          "rgba(0, 240, 255, 0.02)";
                    }}
                    onMouseLeave={(e) => {
                      if (!isSelected)
                        (e.currentTarget as HTMLElement).style.background =
                          "transparent";
                    }}
                  >
                    {pkColumn && (
                      <td className="px-2 py-1.5 w-8">
                        <input
                          type="checkbox"
                          checked={isSelected}
                          onChange={() => toggleRow(rowId)}
                          className="accent-cyan-500"
                          onClick={(e) => e.stopPropagation()}
                        />
                      </td>
                    )}
                    {columnNames.map((name) => {
                      const col = columnMap.get(name);
                      return (
                        <td
                          key={name}
                          className="px-2 py-1.5 whitespace-nowrap cursor-pointer"
                          onClick={() => onRowClick(row)}
                        >
                          <TypeCell
                            value={row[name]}
                            udtName={col?.udt_name ?? "text"}
                          />
                        </td>
                      );
                    })}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {/* Pagination */}
      <div
        className="flex items-center justify-between px-3 py-2 shrink-0"
        style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <div className="flex items-center gap-3">
          <span className="text-[10px] text-slate-600 font-mono">
            {total.toLocaleString()} rows
          </span>
          {selected.size > 0 && (
            <span
              className="text-[10px] font-bold px-2 py-0.5 rounded"
              style={{
                color: "#00f0ff",
                background: "rgba(0, 240, 255, 0.08)",
              }}
            >
              {selected.size} selected
            </span>
          )}
        </div>
        <div className="flex items-center gap-2">
          {/* Page size */}
          <select
            value={limit}
            onChange={(e) => onLimitChange(Number(e.target.value))}
            className="bg-transparent text-[10px] text-slate-400 font-mono rounded px-1.5 py-1 outline-none"
            style={{ border: "1px solid rgba(30, 41, 59, 0.6)" }}
          >
            {PAGE_SIZES.map((s) => (
              <option key={s} value={s} style={{ background: "#111827" }}>
                {s} / page
              </option>
            ))}
          </select>

          {/* Prev */}
          <button
            onClick={() => onPageChange(Math.max(1, page - 1))}
            disabled={page <= 1}
            className="text-[10px] font-mono px-2 py-1 rounded transition-all disabled:opacity-30"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.1)",
            }}
          >
            Prev
          </button>

          <span className="text-[10px] text-slate-500 font-mono">
            {page} / {totalPages}
          </span>

          {/* Next */}
          <button
            onClick={() => onPageChange(Math.min(totalPages, page + 1))}
            disabled={page >= totalPages}
            className="text-[10px] font-mono px-2 py-1 rounded transition-all disabled:opacity-30"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.1)",
            }}
          >
            Next
          </button>
        </div>
      </div>
    </div>
  );
}
