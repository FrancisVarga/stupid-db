"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import {
  uploadDocument,
  searchEmbeddings,
  listEmbeddingDocuments,
  deleteEmbeddingDocument,
  type EmbeddingDocument,
  type SearchResult,
} from "@/lib/api";

// ── Helpers ──────────────────────────────────────────────────────

function formatBytes(bytes: number): string {
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)} MB`;
  if (bytes >= 1_024) return `${(bytes / 1_024).toFixed(1)} KB`;
  return `${bytes} B`;
}

function formatDate(iso: string): string {
  return new Date(iso).toLocaleDateString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  });
}

function similarityColor(score: number): string {
  if (score >= 0.8) return "#06d6a0";
  if (score >= 0.6) return "#ffe600";
  return "#ff4757";
}

const ACCEPTED_TYPES = [".pdf", ".txt", ".md"];

// ── Page ─────────────────────────────────────────────────────────

export default function EmbeddingsPage() {
  const [refreshKey, setRefreshKey] = useState(0);
  const refresh = () => setRefreshKey((k) => k + 1);

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link
            href="/"
            className="text-xs text-slate-500 hover:text-slate-300 transition-colors"
          >
            &larr; Dashboard
          </Link>
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            Embeddings
          </h1>
        </div>
      </header>

      {/* Content */}
      <div className="flex-1 overflow-y-auto px-6 py-5 space-y-6">
        <UploadPanel onUploaded={refresh} />
        <DocumentList refreshKey={refreshKey} onDeleted={refresh} />
        <SearchPanel />
      </div>
    </div>
  );
}

// ── Upload Panel ─────────────────────────────────────────────────

function UploadPanel({ onUploaded }: { onUploaded: () => void }) {
  const fileInputRef = useRef<HTMLInputElement>(null);
  const [dragging, setDragging] = useState(false);
  const [uploading, setUploading] = useState(false);
  const [result, setResult] = useState<{
    filename: string;
    chunk_count: number;
  } | null>(null);
  const [error, setError] = useState<string | null>(null);

  const handleFile = useCallback(
    async (file: File) => {
      const ext = "." + file.name.split(".").pop()?.toLowerCase();
      if (!ACCEPTED_TYPES.includes(ext)) {
        setError(`Unsupported file type: ${ext}. Accepted: ${ACCEPTED_TYPES.join(", ")}`);
        return;
      }
      setError(null);
      setResult(null);
      setUploading(true);
      try {
        const res = await uploadDocument(file);
        setResult({ filename: res.filename, chunk_count: res.chunk_count });
        onUploaded();
      } catch (e) {
        setError(e instanceof Error ? e.message : "Upload failed");
      } finally {
        setUploading(false);
      }
    },
    [onUploaded]
  );

  const onDrop = useCallback(
    (e: React.DragEvent) => {
      e.preventDefault();
      setDragging(false);
      const file = e.dataTransfer.files[0];
      if (file) handleFile(file);
    },
    [handleFile]
  );

  const onFileChange = useCallback(
    (e: React.ChangeEvent<HTMLInputElement>) => {
      const file = e.target.files?.[0];
      if (file) handleFile(file);
      e.target.value = "";
    },
    [handleFile]
  );

  return (
    <div>
      <div
        role="button"
        tabIndex={0}
        className="rounded-xl p-8 text-center cursor-pointer transition-all"
        style={{
          background: dragging
            ? "rgba(0, 240, 255, 0.06)"
            : "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: dragging
            ? "2px dashed rgba(0, 240, 255, 0.5)"
            : "2px dashed rgba(0, 240, 255, 0.15)",
        }}
        onDragOver={(e) => {
          e.preventDefault();
          setDragging(true);
        }}
        onDragLeave={() => setDragging(false)}
        onDrop={onDrop}
        onClick={() => fileInputRef.current?.click()}
        onKeyDown={(e) => {
          if (e.key === "Enter" || e.key === " ") fileInputRef.current?.click();
        }}
      >
        <input
          ref={fileInputRef}
          type="file"
          accept=".pdf,.txt,.md"
          className="hidden"
          onChange={onFileChange}
        />
        {uploading ? (
          <div className="flex items-center justify-center gap-3">
            <span className="w-4 h-4 border-2 border-cyan-400/40 border-t-cyan-400 rounded-full animate-spin" />
            <span className="text-sm text-slate-400">Uploading...</span>
          </div>
        ) : (
          <div>
            <div className="text-sm text-slate-400 mb-1">
              Drop files here or click to browse
            </div>
            <div className="text-[10px] text-slate-600 uppercase tracking-widest">
              PDF, TXT, MD
            </div>
          </div>
        )}
      </div>

      {result && (
        <div
          className="mt-3 px-4 py-2 rounded-lg text-xs"
          style={{
            background: "rgba(6, 214, 160, 0.06)",
            border: "1px solid rgba(6, 214, 160, 0.15)",
            color: "#06d6a0",
          }}
        >
          Uploaded <span className="font-bold">{result.filename}</span> &mdash;{" "}
          {result.chunk_count} chunk{result.chunk_count !== 1 ? "s" : ""} created
        </div>
      )}

      {error && (
        <div
          className="mt-3 px-4 py-2 rounded-lg text-xs"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
            color: "#ff4757",
          }}
        >
          {error}
        </div>
      )}
    </div>
  );
}

// ── Document List ────────────────────────────────────────────────

function DocumentList({
  refreshKey,
  onDeleted,
}: {
  refreshKey: number;
  onDeleted: () => void;
}) {
  const [docs, setDocs] = useState<EmbeddingDocument[]>([]);
  const [loading, setLoading] = useState(true);
  const [deleting, setDeleting] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    listEmbeddingDocuments()
      .then((d) => setDocs(d.documents))
      .catch(() => setDocs([]))
      .finally(() => setLoading(false));
  }, [refreshKey]);

  const handleDelete = async (id: string) => {
    setDeleting(id);
    try {
      await deleteEmbeddingDocument(id);
      onDeleted();
    } catch {
      // silently fail — next refresh will show current state
    } finally {
      setDeleting(null);
    }
  };

  return (
    <div>
      <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
        Documents{!loading && ` (${docs.length})`}
      </h2>

      {loading ? (
        <div className="flex items-center gap-2 py-4">
          <span className="w-3 h-3 border-2 border-cyan-400/40 border-t-cyan-400 rounded-full animate-spin" />
          <span className="text-xs text-slate-500">Loading...</span>
        </div>
      ) : docs.length === 0 ? (
        <div className="text-[10px] text-slate-600 font-mono py-2">
          No documents uploaded yet
        </div>
      ) : (
        <div
          className="rounded-xl overflow-hidden"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <table className="w-full text-[11px]">
            <thead>
              <tr className="border-b border-slate-800">
                <th className="text-left px-4 py-2 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                  Name
                </th>
                <th className="text-left px-4 py-2 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                  Type
                </th>
                <th className="text-right px-4 py-2 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                  Size
                </th>
                <th className="text-right px-4 py-2 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                  Chunks
                </th>
                <th className="text-right px-4 py-2 text-[10px] font-bold uppercase tracking-widest text-slate-500">
                  Uploaded
                </th>
                <th className="w-10" />
              </tr>
            </thead>
            <tbody>
              {docs.map((doc) => (
                <tr
                  key={doc.id}
                  className="border-b border-slate-800/50 last:border-0 hover:bg-slate-800/30 transition-colors"
                >
                  <td className="px-4 py-2 font-mono text-slate-300 truncate max-w-[200px]">
                    {doc.filename}
                  </td>
                  <td className="px-4 py-2 text-slate-500 uppercase">
                    {doc.file_type}
                  </td>
                  <td className="px-4 py-2 text-right font-mono text-slate-400">
                    {formatBytes(doc.file_size)}
                  </td>
                  <td className="px-4 py-2 text-right font-mono" style={{ color: "#00f0ff" }}>
                    {doc.chunk_count}
                  </td>
                  <td className="px-4 py-2 text-right text-slate-500">
                    {formatDate(doc.uploaded_at)}
                  </td>
                  <td className="px-4 py-2 text-right">
                    <button
                      className="text-slate-600 hover:text-red-400 transition-colors disabled:opacity-30"
                      disabled={deleting === doc.id}
                      onClick={() => handleDelete(doc.id)}
                    >
                      {deleting === doc.id ? (
                        <span className="w-3 h-3 border border-red-400/40 border-t-red-400 rounded-full animate-spin inline-block" />
                      ) : (
                        "\u00d7"
                      )}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  );
}

// ── Search Panel ─────────────────────────────────────────────────

function SearchPanel() {
  const [query, setQuery] = useState("");
  const [results, setResults] = useState<SearchResult[]>([]);
  const [searching, setSearching] = useState(false);
  const [searched, setSearched] = useState(false);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);

  const doSearch = async () => {
    const q = query.trim();
    if (!q) return;
    setError(null);
    setSearching(true);
    setSearched(true);
    try {
      const res = await searchEmbeddings(q);
      setResults(res.results);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Search failed");
      setResults([]);
    } finally {
      setSearching(false);
    }
  };

  const toggleExpand = (id: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });
  };

  return (
    <div>
      <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
        Search
      </h2>

      <div className="flex gap-2 mb-4">
        <input
          type="text"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") doSearch();
          }}
          placeholder="Search your documents..."
          className="flex-1 px-4 py-2 rounded-lg text-sm bg-[#0c1018] text-slate-200 placeholder:text-slate-600 outline-none"
          style={{ border: "1px solid rgba(0, 240, 255, 0.15)" }}
        />
        <button
          onClick={doSearch}
          disabled={searching || !query.trim()}
          className="px-5 py-2 rounded-lg text-xs font-bold uppercase tracking-wider disabled:opacity-40 transition-opacity"
          style={{
            background: "rgba(0, 240, 255, 0.1)",
            border: "1px solid rgba(0, 240, 255, 0.25)",
            color: "#00f0ff",
          }}
        >
          {searching ? (
            <span className="w-3 h-3 border-2 border-cyan-400/40 border-t-cyan-400 rounded-full animate-spin inline-block" />
          ) : (
            "Search"
          )}
        </button>
      </div>

      {error && (
        <div
          className="px-4 py-2 rounded-lg text-xs mb-4"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
            color: "#ff4757",
          }}
        >
          {error}
        </div>
      )}

      {!searched ? (
        <div className="text-[10px] text-slate-600 font-mono py-2">
          Upload documents above, then search here.
        </div>
      ) : results.length === 0 && !searching ? (
        <div className="text-[10px] text-slate-600 font-mono py-2">
          No results found
        </div>
      ) : (
        <div className="space-y-2">
          {results.map((r) => {
            const pct = Math.round(r.similarity * 100);
            const color = similarityColor(r.similarity);
            const isExpanded = expanded.has(r.chunk_id);
            const preview =
              r.content.length > 300 && !isExpanded
                ? r.content.slice(0, 300) + "..."
                : r.content;

            return (
              <div
                key={r.chunk_id}
                className="rounded-xl p-4 cursor-pointer transition-all hover:opacity-95"
                style={{
                  background:
                    "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                  border: `1px solid ${color}20`,
                }}
                onClick={() => toggleExpand(r.chunk_id)}
              >
                <div className="flex items-center gap-3 mb-2">
                  <span
                    className="text-[10px] font-mono font-bold px-2 py-0.5 rounded-full"
                    style={{ background: `${color}18`, color }}
                  >
                    {pct}%
                  </span>
                  <span className="text-xs font-mono text-slate-300">
                    {r.filename}
                  </span>
                  {r.page_number != null && (
                    <span className="text-[10px] text-slate-500">
                      p.{r.page_number}
                    </span>
                  )}
                  {r.section_heading && (
                    <span className="text-[10px] text-slate-500">
                      &sect; {r.section_heading}
                    </span>
                  )}
                </div>
                <div className="text-[11px] font-mono text-slate-400 leading-relaxed whitespace-pre-wrap">
                  {preview}
                </div>
                {r.content.length > 300 && (
                  <div
                    className="text-[9px] mt-1 uppercase tracking-widest"
                    style={{ color }}
                  >
                    {isExpanded ? "collapse" : "expand"}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
