"use client";

import { useEffect, useState, use } from "react";
import Link from "next/link";
import DatabaseSidebar from "@/components/db/DatabaseSidebar";
import { fetchTables, type Table } from "@/lib/api-db";

export default function DatabaseDetailPage({
  params,
}: {
  params: Promise<{ db: string }>;
}) {
  const { db } = use(params);
  const [tables, setTables] = useState<Table[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    fetchTables(db)
      .then((t) => {
        setTables(t);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, [db]);

  const totalRows = tables.reduce((sum, t) => sum + t.estimated_rows, 0);

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
          <Link
            href="/db"
            className="text-slate-400 hover:text-slate-200 text-sm font-mono transition-colors"
          >
            Database Manager
          </Link>
          <span className="text-slate-600">/</span>
          <h1 className="text-lg font-bold tracking-wider font-mono" style={{ color: "#00f0ff" }}>
            {decodeURIComponent(db)}
          </h1>
        </div>
        <a
          href={`/api/v1/${encodeURIComponent(db)}/docs`}
          target="_blank"
          rel="noopener noreferrer"
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            background: "rgba(168, 85, 247, 0.1)",
            border: "1px solid rgba(168, 85, 247, 0.3)",
            color: "#a855f7",
          }}
        >
          API Docs
        </a>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 260 }} className="shrink-0">
          <DatabaseSidebar />
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
              <span className="w-2 h-2 rounded-full shrink-0" style={{ background: "#ff4757" }} />
              <span className="text-xs text-red-400 font-mono">{error}</span>
            </div>
          )}

          {/* Loading */}
          {loading && (
            <div className="flex items-center justify-center py-20">
              <span className="text-slate-600 text-sm font-mono animate-pulse">Loading tables...</span>
            </div>
          )}

          {/* Stats */}
          {!loading && !error && (
            <>
              <div className="grid grid-cols-3 gap-4 mb-8">
                <StatCard label="Tables" value={tables.length} accent="#00f0ff" />
                <StatCard label="Total Rows" value={formatNumber(totalRows)} accent="#a855f7" />
                <StatCard
                  label="With Primary Key"
                  value={`${tables.filter((t) => t.has_pk).length}/${tables.length}`}
                  accent="#06d6a0"
                />
              </div>

              <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-4">
                Tables
              </h2>

              {tables.length === 0 ? (
                <div className="flex flex-col items-center justify-center py-16">
                  <p className="text-slate-600 text-sm font-mono">No tables found in this database</p>
                </div>
              ) : (
                <div className="grid grid-cols-2 gap-3">
                  {tables.map((t) => (
                    <Link
                      key={`${t.schema}.${t.name}`}
                      href={`/db/${encodeURIComponent(db)}/${encodeURIComponent(t.name)}`}
                      className="rounded-xl p-4 relative overflow-hidden group transition-all hover:scale-[1.01]"
                      style={{
                        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                        border: "1px solid rgba(0, 240, 255, 0.08)",
                      }}
                    >
                      <div
                        className="absolute top-0 left-0 w-full h-[1px]"
                        style={{
                          background: "linear-gradient(90deg, transparent, rgba(0, 240, 255, 0.3), transparent)",
                        }}
                      />

                      <div className="flex items-center gap-2 mb-2">
                        <svg
                          width="14" height="14" viewBox="0 0 24 24" fill="none"
                          stroke="#00f0ff" strokeWidth="2" className="shrink-0"
                        >
                          <rect x="3" y="3" width="18" height="18" rx="2" />
                          <line x1="3" y1="9" x2="21" y2="9" />
                          <line x1="9" y1="3" x2="9" y2="21" />
                        </svg>
                        <span className="text-sm font-bold font-mono" style={{ color: "#00f0ff" }}>
                          {t.name}
                        </span>
                        {t.schema !== "public" && (
                          <span className="text-[8px] text-slate-600 font-mono px-1.5 py-0.5 rounded"
                            style={{ background: "rgba(100, 116, 139, 0.1)", border: "1px solid rgba(100, 116, 139, 0.15)" }}
                          >
                            {t.schema}
                          </span>
                        )}
                      </div>

                      <div className="flex items-center gap-4 text-[10px] font-mono text-slate-500">
                        <span>~{formatNumber(t.estimated_rows)} rows</span>
                        <span>{t.size}</span>
                        {t.has_pk && (
                          <span
                            className="text-[8px] px-1.5 py-0.5 rounded"
                            style={{ color: "#06d6a0", background: "rgba(6, 214, 160, 0.08)", border: "1px solid rgba(6, 214, 160, 0.15)" }}
                          >
                            PK
                          </span>
                        )}
                      </div>
                    </Link>
                  ))}
                </div>
              )}
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

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}
