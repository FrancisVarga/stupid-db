"use client";

import { useEffect, useState, useRef, useCallback, useMemo } from "react";
import { useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import type { UIMessage } from "ai";
import Link from "next/link";
import ToolCallBlock from "@/components/assistant/ToolCallBlock";
import type { ToolCallBlockProps } from "@/components/assistant/ToolCallBlock";
import {
  fetchSessions,
  createSession,
  deleteSession,
  renameSession,
  type SessionSummary,
} from "@/lib/api";

// ── Tool invocation state mapping ──────────────────────────────────────

function mapToolState(
  sdkState: string,
): ToolCallBlockProps["state"] {
  switch (sdkState) {
    case "input-streaming":
      return "partial-call";
    case "input-available":
    case "approval-requested":
    case "approval-responded":
      return "call";
    case "output-available":
    case "output-error":
    case "output-denied":
      return "result";
    default:
      return "call";
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}

// ── Page Component ─────────────────────────────────────────────────────

export default function AssistantPage() {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [sessionsLoading, setSessionsLoading] = useState(true);
  const [refreshKey, setRefreshKey] = useState(0);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // ── useChat with AI SDK v6 ──────────────────────────────────────────
  // Memoize transport per session so useChat gets a stable reference.
  // The `id` param gives each session its own message store.

  const chatTransport = useMemo(
    () =>
      activeSessionId
        ? new DefaultChatTransport({
            api: "/api/assistant/chat",
            headers: { "X-Session-Id": activeSessionId },
          })
        : null,
    [activeSessionId],
  );

  const { messages, sendMessage, status } = useChat({
    id: activeSessionId ?? undefined,
    transport: chatTransport ?? new DefaultChatTransport({ api: "/api/assistant/chat" }),
  });

  const isLoading = status === "submitted" || status === "streaming";

  // ── Load sessions on mount ──────────────────────────────────────────

  useEffect(() => {
    loadSessions();
  }, []);

  useEffect(() => {
    if (refreshKey === 0) return;
    fetchSessions().then(setSessions).catch(() => {});
  }, [refreshKey]);

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const loadSessions = useCallback(async () => {
    setSessionsLoading(true);
    try {
      const list = await fetchSessions();
      setSessions(list);
      if (list.length > 0 && !activeSessionId) {
        setActiveSessionId(list[0].id);
      } else if (list.length === 0) {
        const session = await createSession("New Chat");
        setSessions([
          {
            ...session,
            message_count: 0,
            last_agent: null,
            last_mode: null,
          },
        ]);
        setActiveSessionId(session.id);
      }
    } catch {
      // Server may not be running
    } finally {
      setSessionsLoading(false);
    }
  }, [activeSessionId]);

  // ── Session handlers ────────────────────────────────────────────────

  const handleNewSession = useCallback(async () => {
    try {
      const session = await createSession("New Chat");
      setActiveSessionId(session.id);
      setRefreshKey((k) => k + 1);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }, []);

  const handleSelectSession = useCallback((id: string) => {
    setActiveSessionId(id);
  }, []);

  const handleRenameSession = useCallback(
    async (id: string, name: string) => {
      try {
        await renameSession(id, name);
        setRefreshKey((k) => k + 1);
      } catch (err) {
        console.error("Failed to rename session:", err);
      }
    },
    [],
  );

  const handleDeleteSession = useCallback(
    async (id: string) => {
      try {
        await deleteSession(id);
        if (activeSessionId === id) {
          const remaining = sessions.filter((s) => s.id !== id);
          if (remaining.length > 0) {
            setActiveSessionId(remaining[0].id);
          } else {
            await handleNewSession();
          }
        }
        setRefreshKey((k) => k + 1);
      } catch (err) {
        console.error("Failed to delete session:", err);
      }
    },
    [activeSessionId, sessions, handleNewSession],
  );

  // ── Submit handler ──────────────────────────────────────────────────

  const [input, setInput] = useState("");

  const handleSubmit = useCallback(
    (e: React.FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      const text = input.trim();
      if (!text || isLoading || !activeSessionId) return;
      sendMessage({ text });
      setInput("");
    },
    [input, isLoading, activeSessionId, sendMessage],
  );

  // ── Render ──────────────────────────────────────────────────────────

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(16, 185, 129, 0.08)",
          background:
            "linear-gradient(180deg, rgba(16, 185, 129, 0.02) 0%, transparent 100%)",
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
            style={{ background: "rgba(16, 185, 129, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#10b981" }}
          >
            Assistant
          </h1>
        </div>
        <div className="flex items-center gap-3">
          <span className="text-[9px] font-mono text-slate-600 uppercase tracking-widest">
            AI SDK v6
          </span>
          {activeSessionId && (
            <span
              className="text-[9px] font-mono px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(16, 185, 129, 0.08)",
                color: "#10b981",
                border: "1px solid rgba(16, 185, 129, 0.15)",
              }}
            >
              {activeSessionId.slice(0, 8)}
            </span>
          )}
        </div>
      </header>

      {/* Body: sidebar + chat */}
      <div className="flex-1 flex min-h-0">
        {/* Session Sidebar */}
        <SessionSidebar
          sessions={sessions}
          activeSessionId={activeSessionId}
          loading={sessionsLoading}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
          onRenameSession={handleRenameSession}
          onDeleteSession={handleDeleteSession}
        />

        {/* Chat Area */}
        <div className="flex-1 flex flex-col min-w-0">
          {/* Messages */}
          <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
            {messages.length === 0 && (
              <div className="flex items-center justify-center h-full">
                <div className="text-center max-w-md">
                  <div
                    className="text-sm font-bold tracking-wider uppercase mb-2"
                    style={{ color: "#10b981" }}
                  >
                    Assistant Chat
                  </div>
                  <p className="text-slate-500 text-xs leading-relaxed font-mono">
                    Ask questions about your data. The assistant can execute
                    tools, query graphs, evaluate rules, and read files.
                  </p>
                </div>
              </div>
            )}

            {messages.map((msg) => (
              <MessageRow key={msg.id} message={msg} />
            ))}

            {isLoading && messages.length > 0 && (
              <div className="flex items-center gap-3 px-4 py-3">
                <Spinner />
                <span className="text-xs text-slate-500 animate-pulse font-mono">
                  {status === "submitted"
                    ? "Sending..."
                    : "Assistant is thinking..."}
                </span>
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input bar */}
          <form
            onSubmit={handleSubmit}
            className="px-6 py-4 shrink-0"
            style={{
              borderTop: "1px solid rgba(16, 185, 129, 0.08)",
              background:
                "linear-gradient(0deg, rgba(16, 185, 129, 0.02) 0%, transparent 100%)",
            }}
          >
            <div
              className="flex items-center gap-3 rounded-xl px-4 py-3"
              style={{
                background: "rgba(16, 185, 129, 0.03)",
                border: "1px solid rgba(16, 185, 129, 0.1)",
              }}
            >
              <input
                ref={inputRef}
                type="text"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder="Ask the assistant..."
                disabled={isLoading || !activeSessionId}
                className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-600 outline-none font-mono"
              />
              <button
                type="submit"
                disabled={isLoading || !input.trim() || !activeSessionId}
                className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-all disabled:opacity-30"
                style={{
                  background: "rgba(16, 185, 129, 0.15)",
                  border: "1px solid rgba(16, 185, 129, 0.3)",
                  color: "#10b981",
                }}
              >
                {isLoading ? "Running..." : "Send"}
              </button>
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}

// ── Message Row ───────────────────────────────────────────────────────

function MessageRow({ message }: { message: UIMessage }) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-[70%] rounded-xl px-4 py-3"
          style={{
            background: "rgba(16, 185, 129, 0.08)",
            border: "1px solid rgba(16, 185, 129, 0.15)",
          }}
        >
          {message.parts.map((part, i) => {
            if (part.type === "text") {
              return (
                <p
                  key={i}
                  className="text-sm text-slate-200 font-mono whitespace-pre-wrap"
                >
                  {part.text}
                </p>
              );
            }
            return null;
          })}
        </div>
      </div>
    );
  }

  // Assistant message
  return (
    <div className="flex justify-start">
      <div
        className="max-w-[85%] rounded-xl px-4 py-3"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: "1px solid rgba(16, 185, 129, 0.1)",
        }}
      >
        <div className="flex items-center gap-2 mb-2">
          <span
            className="text-[10px] font-bold font-mono tracking-wider"
            style={{ color: "#10b981" }}
          >
            ASSISTANT
          </span>
        </div>

        {message.parts.map((part, i) => {
          if (part.type === "text") {
            return (
              <p
                key={i}
                className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed"
              >
                {part.text}
              </p>
            );
          }

          if (part.type === "dynamic-tool") {
            return (
              <ToolCallBlock
                key={`${part.toolCallId}-${i}`}
                toolName={part.toolName}
                toolCallId={part.toolCallId}
                args={
                  (part.input as Record<string, unknown>) ?? {}
                }
                result={
                  part.state === "output-available"
                    ? part.output
                    : part.state === "output-error"
                      ? part.errorText
                      : undefined
                }
                state={mapToolState(part.state)}
              />
            );
          }

          if (part.type === "step-start") {
            return (
              <div
                key={i}
                className="my-2 h-[1px]"
                style={{
                  background:
                    "linear-gradient(90deg, transparent, rgba(16, 185, 129, 0.15), transparent)",
                }}
              />
            );
          }

          return null;
        })}
      </div>
    </div>
  );
}

// ── Session Sidebar ───────────────────────────────────────────────────

function SessionSidebar({
  sessions,
  activeSessionId,
  loading,
  onSelectSession,
  onNewSession,
  onRenameSession,
  onDeleteSession,
}: {
  sessions: SessionSummary[];
  activeSessionId: string | null;
  loading: boolean;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onRenameSession: (id: string, name: string) => void;
  onDeleteSession: (id: string) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const startRename = (id: string, currentName: string) => {
    setEditingId(id);
    setEditName(currentName);
  };

  const commitRename = (id: string) => {
    if (editName.trim()) {
      onRenameSession(id, editName.trim());
    }
    setEditingId(null);
  };

  return (
    <div
      className="w-60 shrink-0 flex flex-col overflow-hidden"
      style={{
        borderRight: "1px solid rgba(16, 185, 129, 0.06)",
        background: "rgba(0, 0, 0, 0.15)",
      }}
    >
      {/* New session button */}
      <button
        onClick={onNewSession}
        className="m-3 px-3 py-2 rounded-lg text-xs font-bold tracking-wider uppercase transition-all hover:opacity-90"
        style={{
          background: "rgba(16, 185, 129, 0.1)",
          border: "1px solid rgba(16, 185, 129, 0.2)",
          color: "#10b981",
        }}
      >
        + New Chat
      </button>

      {/* Loading */}
      {loading && (
        <div className="px-3 py-4 text-center">
          <span className="text-slate-600 text-xs font-mono animate-pulse">
            Loading sessions...
          </span>
        </div>
      )}

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-2 pb-2 space-y-1">
        {sessions.map((s) => {
          const isActive = s.id === activeSessionId;
          const isEditing = s.id === editingId;
          const isConfirmingDelete = s.id === confirmDeleteId;

          return (
            <div
              key={s.id}
              className="group rounded-lg px-3 py-2.5 cursor-pointer transition-all"
              style={{
                background: isActive
                  ? "rgba(16, 185, 129, 0.06)"
                  : "transparent",
                borderLeft: isActive
                  ? "2px solid #10b981"
                  : "2px solid transparent",
              }}
              onClick={() => !isEditing && onSelectSession(s.id)}
            >
              {isEditing ? (
                <input
                  autoFocus
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  onBlur={() => commitRename(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename(s.id);
                    if (e.key === "Escape") setEditingId(null);
                  }}
                  onClick={(e) => e.stopPropagation()}
                  className="w-full bg-transparent text-xs text-slate-200 font-mono outline-none px-1 py-0.5 rounded"
                  style={{
                    border: "1px solid rgba(16, 185, 129, 0.3)",
                  }}
                />
              ) : (
                <>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-300 font-mono truncate flex-1">
                      {s.name}
                    </span>
                    {/* Hover actions */}
                    <div className="hidden group-hover:flex items-center gap-1 ml-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          startRename(s.id, s.name);
                        }}
                        className="text-[9px] text-slate-500 hover:text-slate-300 px-1"
                        title="Rename"
                      >
                        &#9998;
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          if (isConfirmingDelete) {
                            onDeleteSession(s.id);
                            setConfirmDeleteId(null);
                          } else {
                            setConfirmDeleteId(s.id);
                            setTimeout(() => setConfirmDeleteId(null), 3000);
                          }
                        }}
                        className="text-[9px] px-1"
                        style={{
                          color: isConfirmingDelete ? "#ff4757" : "#64748b",
                        }}
                        title={
                          isConfirmingDelete
                            ? "Click again to confirm"
                            : "Delete"
                        }
                      >
                        &#10005;
                      </button>
                    </div>
                  </div>
                  <div className="flex items-center gap-2 mt-1">
                    {s.message_count > 0 && (
                      <span
                        className="text-[9px] font-mono px-1 rounded"
                        style={{
                          background: "rgba(16, 185, 129, 0.08)",
                          color: "#64748b",
                        }}
                      >
                        {s.message_count} msg
                      </span>
                    )}
                    <span className="text-[9px] text-slate-600 font-mono ml-auto">
                      {relativeTime(s.updated_at)}
                    </span>
                  </div>
                </>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

// ── Spinner ───────────────────────────────────────────────────────────

function Spinner() {
  return (
    <div
      className="w-4 h-4 rounded-full animate-spin"
      style={{
        border: "2px solid rgba(16, 185, 129, 0.1)",
        borderTopColor: "#10b981",
      }}
    />
  );
}
