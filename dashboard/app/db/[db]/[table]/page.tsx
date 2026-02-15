"use client";

import { useEffect, useState, useCallback, use } from "react";
import Link from "next/link";
import DatabaseSidebar from "@/components/db/DatabaseSidebar";
import DataGrid from "@/components/db/DataGrid";
import RecordForm from "@/components/db/RecordForm";
import QueryPanel from "@/components/db/QueryPanel";
import {
  fetchTableSchema,
  fetchRows,
  fetchAuditLog,
  createRow,
  updateRow,
  deleteRow,
  batchDelete,
  type Column,
  type AuditEntry,
} from "@/lib/api-db";

type ViewTab = "data" | "query" | "audit";

const AUDIT_OP_COLORS: Record<string, string> = {
  list: "#06d6a0",
  get: "#06d6a0",
  create: "#00d4ff",
  update: "#ffe600",
  delete: "#ff4757",
  batch_update: "#ffe600",
  batch_delete: "#ff4757",
  query: "#64748b",
};

export default function TableViewPage({
  params,
}: {
  params: Promise<{ db: string; table: string }>;
}) {
  const { db, table } = use(params);

  // Schema
  const [columns, setColumns] = useState<Column[]>([]);
  const [schemaLoading, setSchemaLoading] = useState(true);

  // Data
  const [rows, setRows] = useState<Record<string, unknown>[]>([]);
  const [total, setTotal] = useState(0);
  const [page, setPage] = useState(1);
  const [limit, setLimit] = useState(50);
  const [sortCol, setSortCol] = useState<string | null>(null);
  const [sortOrder, setSortOrder] = useState<"asc" | "desc">("asc");
  const [dataLoading, setDataLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Selection
  const [selectedIds, setSelectedIds] = useState<string[]>([]);

  // UI state
  const [activeTab, setActiveTab] = useState<ViewTab>("data");
  const [showForm, setShowForm] = useState(false);
  const [editRow, setEditRow] = useState<Record<string, unknown> | null>(null);

  // Audit
  const [auditEntries, setAuditEntries] = useState<AuditEntry[]>([]);
  const [auditTotal, setAuditTotal] = useState(0);
  const [auditPage, setAuditPage] = useState(1);

  // Find PK column
  const pkColumn =
    columns.find((c) => c.is_pk)?.name ?? null;

  // Load schema on mount
  useEffect(() => {
    setSchemaLoading(true);
    fetchTableSchema(db, table)
      .then((cols) => {
        setColumns(cols);
        setSchemaLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setSchemaLoading(false);
      });
  }, [db, table]);

  // Load data when page/sort/limit changes
  const loadData = useCallback(async () => {
    setDataLoading(true);
    setError(null);
    try {
      const res = await fetchRows(db, table, {
        page,
        limit,
        sort: sortCol ?? undefined,
        order: sortCol ? sortOrder : undefined,
      });
      setRows(res.rows);
      setTotal(res.total);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setDataLoading(false);
    }
  }, [db, table, page, limit, sortCol, sortOrder]);

  useEffect(() => {
    if (!schemaLoading) loadData();
  }, [loadData, schemaLoading]);

  // Load audit when tab changes
  useEffect(() => {
    if (activeTab !== "audit") return;
    fetchAuditLog(db, { table, page: auditPage, limit: 50 })
      .then((res) => {
        setAuditEntries(res.rows);
        setAuditTotal(res.total);
      })
      .catch(() => {});
  }, [activeTab, auditPage, db, table]);

  // Handlers
  const handleSort = useCallback(
    (col: string) => {
      if (sortCol === col) {
        setSortOrder((prev) => (prev === "asc" ? "desc" : "asc"));
      } else {
        setSortCol(col);
        setSortOrder("asc");
      }
      setPage(1);
    },
    [sortCol]
  );

  const handleRowClick = useCallback((row: Record<string, unknown>) => {
    setEditRow(row);
    setShowForm(true);
  }, []);

  const handleCreate = useCallback(
    async (data: Record<string, unknown>) => {
      await createRow(db, table, data);
      setShowForm(false);
      loadData();
    },
    [db, table, loadData]
  );

  const handleUpdate = useCallback(
    async (data: Record<string, unknown>) => {
      if (!pkColumn || !editRow) return;
      const id = String(editRow[pkColumn]);
      await updateRow(db, table, id, data);
      setShowForm(false);
      setEditRow(null);
      loadData();
    },
    [db, table, pkColumn, editRow, loadData]
  );

  const handleDelete = useCallback(
    async (id: string) => {
      if (!confirm("Delete this record?")) return;
      await deleteRow(db, table, id);
      setShowForm(false);
      setEditRow(null);
      loadData();
    },
    [db, table, loadData]
  );

  const handleBatchDelete = useCallback(async () => {
    if (selectedIds.length === 0) return;
    if (
      !confirm(`Delete ${selectedIds.length} record${selectedIds.length > 1 ? "s" : ""}?`)
    )
      return;
    try {
      await batchDelete(db, table, selectedIds);
      setSelectedIds([]);
      loadData();
    } catch (e) {
      setError((e as Error).message);
    }
  }, [db, table, selectedIds, loadData]);

  const tabs: { key: ViewTab; label: string }[] = [
    { key: "data", label: "Data" },
    { key: "query", label: "Query" },
    { key: "audit", label: "Audit" },
  ];

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div
            className="w-[1px] h-4"
            style={{ background: "rgba(0, 240, 255, 0.12)" }}
          />
          <Link
            href="/db"
            className="text-slate-400 hover:text-slate-200 text-sm font-mono transition-colors"
          >
            {db}
          </Link>
          <span className="text-slate-600">/</span>
          <h1
            className="text-lg font-bold tracking-wider font-mono"
            style={{ color: "#00f0ff" }}
          >
            {table}
          </h1>
          <span className="text-[10px] text-slate-600 font-mono">
            {total.toLocaleString()} rows
          </span>
        </div>

        <div className="flex items-center gap-2">
          {/* Batch delete */}
          {selectedIds.length > 0 && (
            <button
              onClick={handleBatchDelete}
              className="px-3 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90"
              style={{
                color: "#ff4757",
                background: "rgba(255, 71, 87, 0.08)",
                border: "1px solid rgba(255, 71, 87, 0.2)",
              }}
            >
              Delete {selectedIds.length}
            </button>
          )}

          {/* New record */}
          <button
            onClick={() => {
              setEditRow(null);
              setShowForm(true);
            }}
            className="px-3 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90"
            style={{
              color: "#06080d",
              background: "#00f0ff",
            }}
          >
            + New Record
          </button>

          {/* Docs link */}
          <a
            href={`/api/v1/${encodeURIComponent(db)}/docs`}
            target="_blank"
            rel="noopener noreferrer"
            className="px-3 py-1.5 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-80"
            style={{
              color: "#a855f7",
              background: "rgba(168, 85, 247, 0.08)",
              border: "1px solid rgba(168, 85, 247, 0.2)",
            }}
          >
            API Docs
          </a>
        </div>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        {/* Sidebar */}
        <div style={{ width: 260 }} className="shrink-0">
          <DatabaseSidebar />
        </div>

        {/* Main content */}
        <div className="flex-1 flex flex-col min-h-0">
          {/* Tabs */}
          <div
            className="flex shrink-0"
            style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.08)" }}
          >
            {tabs.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setActiveTab(tab.key)}
                className="px-6 py-2.5 text-[10px] font-bold tracking-[0.15em] uppercase transition-all"
                style={{
                  color: activeTab === tab.key ? "#00f0ff" : "#475569",
                  background:
                    activeTab === tab.key
                      ? "rgba(0, 240, 255, 0.05)"
                      : "transparent",
                  borderBottom:
                    activeTab === tab.key
                      ? "2px solid #00f0ff"
                      : "2px solid transparent",
                }}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Error */}
          {error && (
            <div
              className="mx-4 mt-3 flex items-center gap-3 px-4 py-2.5 rounded-lg"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
              }}
            >
              <span
                className="w-2 h-2 rounded-full shrink-0"
                style={{ background: "#ff4757" }}
              />
              <span className="text-xs text-red-400 font-mono">{error}</span>
            </div>
          )}

          {/* Tab content */}
          <div className="flex-1 min-h-0">
            {activeTab === "data" && (
              <>
                {schemaLoading || dataLoading ? (
                  <div className="flex items-center justify-center h-full">
                    <span className="text-slate-600 text-sm font-mono animate-pulse">
                      Loading data...
                    </span>
                  </div>
                ) : (
                  <DataGrid
                    rows={rows}
                    columns={columns}
                    total={total}
                    page={page}
                    limit={limit}
                    sortCol={sortCol}
                    sortOrder={sortOrder}
                    onSort={handleSort}
                    onPageChange={setPage}
                    onLimitChange={(l) => {
                      setLimit(l);
                      setPage(1);
                    }}
                    onRowClick={handleRowClick}
                    onSelectionChange={setSelectedIds}
                    pkColumn={pkColumn}
                  />
                )}
              </>
            )}

            {activeTab === "query" && <QueryPanel db={db} />}

            {activeTab === "audit" && (
              <AuditView
                entries={auditEntries}
                total={auditTotal}
                page={auditPage}
                onPageChange={setAuditPage}
              />
            )}
          </div>
        </div>
      </div>

      {/* Record form modal */}
      {showForm && (
        <RecordForm
          columns={columns}
          initialData={editRow ?? undefined}
          mode={editRow ? "edit" : "create"}
          onSubmit={editRow ? handleUpdate : handleCreate}
          onClose={() => {
            setShowForm(false);
            setEditRow(null);
          }}
        />
      )}

      {/* Delete button in edit mode */}
      {showForm && editRow && pkColumn && (
        <button
          onClick={() => handleDelete(String(editRow[pkColumn]))}
          className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 px-4 py-2 text-[10px] font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90"
          style={{
            color: "#ff4757",
            background: "rgba(255, 71, 87, 0.1)",
            border: "1px solid rgba(255, 71, 87, 0.3)",
          }}
        >
          Delete Record
        </button>
      )}
    </div>
  );
}

// ── Audit View ────────────────────────────────────────────────────────

function AuditView({
  entries,
  total,
  page,
  onPageChange,
}: {
  entries: AuditEntry[];
  total: number;
  page: number;
  onPageChange: (p: number) => void;
}) {
  const totalPages = Math.max(1, Math.ceil(total / 50));

  return (
    <div className="flex flex-col h-full">
      <div className="flex-1 overflow-auto">
        {entries.length === 0 ? (
          <div className="flex items-center justify-center h-full">
            <span className="text-slate-600 text-sm font-mono">
              No audit entries yet
            </span>
          </div>
        ) : (
          <table className="w-full text-[10px] font-mono">
            <thead className="sticky top-0" style={{ background: "#0c1018" }}>
              <tr style={{ borderBottom: "1px solid #1e293b" }}>
                <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
                  Time
                </th>
                <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
                  Operation
                </th>
                <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
                  Method
                </th>
                <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
                  Record
                </th>
                <th className="px-3 py-2 text-right text-slate-500 font-bold tracking-wider uppercase">
                  Status
                </th>
                <th className="px-3 py-2 text-right text-slate-500 font-bold tracking-wider uppercase">
                  Duration
                </th>
                <th className="px-3 py-2 text-left text-slate-500 font-bold tracking-wider uppercase">
                  IP
                </th>
              </tr>
            </thead>
            <tbody>
              {entries.map((entry) => {
                const opColor =
                  AUDIT_OP_COLORS[entry.operation] || "#64748b";
                return (
                  <tr
                    key={entry.id}
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
                    <td className="px-3 py-1.5 text-slate-500 whitespace-nowrap">
                      {new Date(entry.timestamp).toLocaleString()}
                    </td>
                    <td className="px-3 py-1.5">
                      <span
                        className="text-[9px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
                        style={{
                          color: opColor,
                          background: `${opColor}12`,
                          border: `1px solid ${opColor}25`,
                        }}
                      >
                        {entry.operation}
                      </span>
                    </td>
                    <td className="px-3 py-1.5 text-slate-400">
                      {entry.method}
                    </td>
                    <td className="px-3 py-1.5 text-slate-400 truncate max-w-[120px]">
                      {entry.record_id || (entry.record_ids?.join(", ") ?? "-")}
                    </td>
                    <td className="px-3 py-1.5 text-right">
                      <span
                        style={{
                          color:
                            entry.response_status < 400
                              ? "#06d6a0"
                              : "#ff4757",
                        }}
                      >
                        {entry.response_status}
                      </span>
                    </td>
                    <td
                      className="px-3 py-1.5 text-right font-bold"
                      style={{ color: "#a855f7" }}
                    >
                      {entry.duration_ms.toFixed(1)}ms
                    </td>
                    <td className="px-3 py-1.5 text-slate-600 truncate max-w-[100px]">
                      {entry.ip || "-"}
                    </td>
                  </tr>
                );
              })}
            </tbody>
          </table>
        )}
      </div>

      {/* Pagination */}
      {total > 0 && (
        <div
          className="flex items-center justify-between px-3 py-2 shrink-0"
          style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          <span className="text-[10px] text-slate-600 font-mono">
            {total.toLocaleString()} entries
          </span>
          <div className="flex items-center gap-2">
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
      )}
    </div>
  );
}
