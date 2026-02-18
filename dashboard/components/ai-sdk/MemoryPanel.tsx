"use client";

import { useState, useEffect, useCallback, useRef } from "react";

// ── Types ─────────────────────────────────────────────────────────────

interface Memory {
  id: string;
  content: string;
  category: string | null;
  tags: string[];
  created_at: string;
  updated_at: string;
}

interface MemoryPanelProps {
  open: boolean;
  onClose: () => void;
}

// ── API helpers ───────────────────────────────────────────────────────

async function fetchMemories(opts?: {
  tag?: string;
  category?: string;
}): Promise<Memory[]> {
  const params = new URLSearchParams();
  if (opts?.tag) params.set("tag", opts.tag);
  if (opts?.category) params.set("category", opts.category);
  const qs = params.toString();
  const res = await fetch(`/api/ai-sdk/memories${qs ? `?${qs}` : ""}`);
  if (!res.ok) throw new Error("Failed to load memories");
  return res.json();
}

async function searchMemories(q: string): Promise<Memory[]> {
  const res = await fetch(
    `/api/ai-sdk/memories/search?q=${encodeURIComponent(q)}`,
  );
  if (!res.ok) throw new Error("Search failed");
  return res.json();
}

async function deleteMemory(id: string): Promise<void> {
  const res = await fetch(`/api/ai-sdk/memories/${id}`, { method: "DELETE" });
  if (!res.ok) throw new Error("Delete failed");
}

// ── Component ─────────────────────────────────────────────────────────

export default function MemoryPanel({ open, onClose }: MemoryPanelProps) {
  const [memories, setMemories] = useState<Memory[]>([]);
  const [loading, setLoading] = useState(false);
  const [searchQuery, setSearchQuery] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const searchTimeoutRef = useRef<ReturnType<typeof setTimeout>>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const loadMemories = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchMemories();
      setMemories(data);
    } catch (err) {
      setError(err instanceof Error ? err.message : "Failed to load");
    } finally {
      setLoading(false);
    }
  }, []);

  // Load memories when panel opens
  useEffect(() => {
    if (open) loadMemories();
  }, [open, loadMemories]);

  // Debounced search
  useEffect(() => {
    if (!open) return;
    if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);

    if (!searchQuery.trim()) {
      loadMemories();
      return;
    }

    searchTimeoutRef.current = setTimeout(async () => {
      setLoading(true);
      setError(null);
      try {
        const data = await searchMemories(searchQuery);
        setMemories(data);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Search failed");
      } finally {
        setLoading(false);
      }
    }, 300);

    return () => {
      if (searchTimeoutRef.current) clearTimeout(searchTimeoutRef.current);
    };
  }, [searchQuery, open, loadMemories]);

  const handleDelete = useCallback(
    async (id: string) => {
      try {
        await deleteMemory(id);
        setMemories((prev) => prev.filter((m) => m.id !== id));
        setConfirmDeleteId(null);
      } catch (err) {
        setError(err instanceof Error ? err.message : "Delete failed");
      }
    },
    [],
  );

  if (!open) return null;

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40"
        style={{ background: "rgba(0, 0, 0, 0.4)" }}
        onClick={onClose}
      />

      {/* Slide-out panel */}
      <div
        ref={panelRef}
        className="fixed right-0 top-0 bottom-0 z-50 w-96 flex flex-col overflow-hidden"
        style={{
          background: "linear-gradient(180deg, #0c1018 0%, #111827 100%)",
          borderLeft: "1px solid rgba(139, 92, 246, 0.1)",
          boxShadow: "-8px 0 32px rgba(0, 0, 0, 0.5)",
        }}
      >
        {/* Header */}
        <div
          className="px-5 py-4 shrink-0 flex items-center justify-between"
          style={{
            borderBottom: "1px solid rgba(139, 92, 246, 0.08)",
            background:
              "linear-gradient(180deg, rgba(139, 92, 246, 0.03) 0%, transparent 100%)",
          }}
        >
          <div className="flex items-center gap-3">
            <span
              className="text-sm font-bold tracking-wider uppercase"
              style={{ color: "#8b5cf6" }}
            >
              Memories
            </span>
            <span
              className="text-[10px] font-mono px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(139, 92, 246, 0.1)",
                color: "#a78bfa",
              }}
            >
              {memories.length}
            </span>
          </div>
          <button
            onClick={onClose}
            className="text-slate-500 hover:text-slate-300 transition-colors text-lg leading-none px-1"
            title="Close"
          >
            &#10005;
          </button>
        </div>

        {/* Search bar */}
        <div className="px-4 py-3 shrink-0">
          <div
            className="flex items-center gap-2 rounded-lg px-3 py-2"
            style={{
              background: "rgba(139, 92, 246, 0.03)",
              border: "1px solid rgba(139, 92, 246, 0.1)",
            }}
          >
            <svg
              width="14"
              height="14"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              strokeLinecap="round"
              strokeLinejoin="round"
              className="text-slate-500 shrink-0"
            >
              <circle cx="11" cy="11" r="8" />
              <line x1="21" y1="21" x2="16.65" y2="16.65" />
            </svg>
            <input
              type="text"
              value={searchQuery}
              onChange={(e) => setSearchQuery(e.target.value)}
              placeholder="Search memories..."
              className="flex-1 bg-transparent text-xs text-slate-200 placeholder:text-slate-600 outline-none font-mono"
            />
            {searchQuery && (
              <button
                onClick={() => setSearchQuery("")}
                className="text-slate-500 hover:text-slate-300 text-xs"
              >
                &#10005;
              </button>
            )}
          </div>
        </div>

        {/* Error banner */}
        {error && (
          <div
            className="mx-4 mb-2 px-3 py-2 rounded-lg text-xs font-mono"
            style={{
              background: "rgba(239, 68, 68, 0.08)",
              border: "1px solid rgba(239, 68, 68, 0.2)",
              color: "#f87171",
            }}
          >
            {error}
          </div>
        )}

        {/* Memory list */}
        <div className="flex-1 overflow-y-auto px-4 pb-4 space-y-2">
          {loading && memories.length === 0 && (
            <div className="flex items-center justify-center py-12">
              <Spinner />
              <span className="ml-2 text-xs text-slate-500 font-mono animate-pulse">
                Loading memories...
              </span>
            </div>
          )}

          {!loading && memories.length === 0 && <EmptyMemories />}

          {memories.map((memory) => (
            <MemoryCard
              key={memory.id}
              memory={memory}
              confirmDelete={confirmDeleteId === memory.id}
              onDelete={() => {
                if (confirmDeleteId === memory.id) {
                  handleDelete(memory.id);
                } else {
                  setConfirmDeleteId(memory.id);
                  setTimeout(() => setConfirmDeleteId(null), 3000);
                }
              }}
            />
          ))}
        </div>
      </div>
    </>
  );
}

// ── Memory Card ───────────────────────────────────────────────────────

function MemoryCard({
  memory,
  confirmDelete,
  onDelete,
}: {
  memory: Memory;
  confirmDelete: boolean;
  onDelete: () => void;
}) {
  return (
    <div
      className="group rounded-lg px-4 py-3 transition-all"
      style={{
        background: "rgba(139, 92, 246, 0.02)",
        border: "1px solid rgba(139, 92, 246, 0.06)",
      }}
    >
      {/* Content */}
      <p className="text-xs text-slate-300 font-mono leading-relaxed whitespace-pre-wrap">
        {memory.content}
      </p>

      {/* Badges row */}
      <div className="flex items-center gap-1.5 mt-2 flex-wrap">
        {memory.category && (
          <span
            className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
            style={{
              background: "rgba(139, 92, 246, 0.12)",
              color: "#a78bfa",
            }}
          >
            {memory.category}
          </span>
        )}
        {memory.tags.map((tag) => (
          <span
            key={tag}
            className="text-[9px] font-mono px-1.5 py-0.5 rounded"
            style={{
              background: "rgba(56, 189, 248, 0.08)",
              color: "#7dd3fc",
              border: "1px solid rgba(56, 189, 248, 0.15)",
            }}
          >
            {tag}
          </span>
        ))}
      </div>

      {/* Footer: timestamp + delete */}
      <div className="flex items-center justify-between mt-2">
        <span className="text-[9px] text-slate-600 font-mono">
          {relativeTime(memory.created_at)}
        </span>
        <button
          onClick={onDelete}
          className="opacity-0 group-hover:opacity-100 transition-opacity text-[9px] font-mono px-1.5 py-0.5 rounded"
          style={{
            color: confirmDelete ? "#ff4757" : "#64748b",
            background: confirmDelete
              ? "rgba(255, 71, 87, 0.1)"
              : "transparent",
          }}
          title={confirmDelete ? "Click again to confirm" : "Delete memory"}
        >
          {confirmDelete ? "confirm?" : "delete"}
        </button>
      </div>
    </div>
  );
}

// ── Empty state ───────────────────────────────────────────────────────

function EmptyMemories() {
  return (
    <div className="flex items-center justify-center py-16">
      <div className="text-center max-w-xs">
        <div
          className="text-[11px] font-bold tracking-wider uppercase mb-2"
          style={{ color: "#7c3aed" }}
        >
          No Memories
        </div>
        <p className="text-slate-600 text-[10px] font-mono leading-relaxed">
          The AI will save memories during conversations when it learns important
          facts, preferences, or context. They&apos;ll appear here.
        </p>
      </div>
    </div>
  );
}

// ── Helpers ───────────────────────────────────────────────────────────

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

function Spinner() {
  return (
    <div
      className="w-4 h-4 rounded-full animate-spin"
      style={{
        border: "2px solid rgba(139, 92, 246, 0.1)",
        borderTopColor: "#8b5cf6",
      }}
    />
  );
}
