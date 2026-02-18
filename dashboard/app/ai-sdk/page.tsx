"use client";

import { useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import type { UIMessage } from "ai";
import Link from "next/link";
import {
  useState,
  useRef,
  useEffect,
  useCallback,
  useMemo,
} from "react";
import type { ChatMetadata, ChatUIMessage } from "../api/ai-sdk/chat/route";
import {
  listSessions,
  createSession,
  updateSession,
  deleteSession,
  type AiSdkSession,
} from "@/lib/ai-sdk/sessions";
import ToolCallBlock from "@/components/ai-sdk/ToolCallBlock";
import type { ToolCallBlockProps } from "@/components/ai-sdk/ToolCallBlock";
import MemoryPanel from "@/components/ai-sdk/MemoryPanel";

// ── Tool invocation state mapping ──────────────────────────────────────

function mapToolState(sdkState: string): ToolCallBlockProps["state"] {
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

// ── Constants ────────────────────────────────────────────────────────

const PROVIDERS = [
  { id: "anthropic", label: "Anthropic API" },
  { id: "claude-code", label: "Claude Code CLI" },
] as const;

type ProviderType = (typeof PROVIDERS)[number]["id"];

const MODELS_BY_PROVIDER: Record<ProviderType, { id: string; label: string }[]> = {
  anthropic: [
    { id: "claude-sonnet-4-6", label: "Sonnet 4.6" },
    { id: "claude-opus-4-6", label: "Opus 4.6" },
    { id: "claude-sonnet-4-5", label: "Sonnet 4.5" },
    { id: "claude-opus-4-5", label: "Opus 4.5" },
    { id: "claude-haiku-4-5", label: "Haiku 4.5" },
  ],
  "claude-code": [
    { id: "cc-sonnet", label: "Sonnet (via CLI)" },
    { id: "cc-opus", label: "Opus (via CLI)" },
    { id: "cc-haiku", label: "Haiku (via CLI)" },
  ],
};

// ── Page ─────────────────────────────────────────────────────────────

export default function AiSdkPage() {
  const [selectedProvider, setSelectedProvider] = useState<ProviderType>("anthropic");
  const [selectedModel, setSelectedModel] = useState<string>(
    MODELS_BY_PROVIDER.anthropic[0].id,
  );
  const [input, setInput] = useState("");
  const [files, setFiles] = useState<FileList | undefined>(undefined);
  const [memoryPanelOpen, setMemoryPanelOpen] = useState(false);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // ── Session state ───────────────────────────────────────────────────
  const [sessions, setSessions] = useState<AiSdkSession[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [sessionsLoading, setSessionsLoading] = useState(true);
  const [refreshKey, setRefreshKey] = useState(0);

  // Load sessions on mount
  useEffect(() => {
    loadSessions();
  // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  // Refresh sessions when refreshKey changes (refreshKey pattern)
  useEffect(() => {
    if (refreshKey === 0) return;
    listSessions().then(setSessions).catch(() => {});
  }, [refreshKey]);

  const loadSessions = useCallback(async () => {
    setSessionsLoading(true);
    try {
      const list = await listSessions();
      setSessions(list);
      if (list.length > 0 && !activeSessionId) {
        setActiveSessionId(list[0].id);
      }
    } catch {
      // DB may not be available yet
    } finally {
      setSessionsLoading(false);
    }
  }, [activeSessionId]);

  const handleNewSession = useCallback(async () => {
    try {
      const session = await createSession({
        provider: selectedProvider,
        model: selectedModel,
      });
      setActiveSessionId(session.id);
      setRefreshKey((k) => k + 1);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }, [selectedProvider, selectedModel]);

  const handleSelectSession = useCallback((id: string) => {
    setActiveSessionId(id);
  }, []);

  const handleRenameSession = useCallback(
    async (id: string, title: string) => {
      try {
        await updateSession(id, { title });
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
            setActiveSessionId(null);
          }
        }
        setRefreshKey((k) => k + 1);
      } catch (err) {
        console.error("Failed to delete session:", err);
      }
    },
    [activeSessionId, sessions],
  );

  // Update model when provider changes
  useEffect(() => {
    const models = MODELS_BY_PROVIDER[selectedProvider];
    setSelectedModel(models[0].id);
  }, [selectedProvider]);

  const transport = useMemo(
    () =>
      new DefaultChatTransport({
        api: "/api/ai-sdk/chat",
        body: {
          model: selectedModel,
          provider: selectedProvider,
          sessionId: activeSessionId,
        },
      }),
    [selectedModel, selectedProvider, activeSessionId],
  );

  const {
    messages,
    sendMessage,
    status,
    stop,
    regenerate,
    error,
  } = useChat<ChatUIMessage>({
    id: activeSessionId ?? undefined,
    transport,
  });

  const isActive = status === "submitted" || status === "streaming";

  // Auto-scroll
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSubmit = useCallback(
    (e: React.FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      const text = input.trim();
      if (!text && !files?.length) return;
      if (isActive) return;
      sendMessage({ text: text || " ", files });
      setInput("");
      setFiles(undefined);
      if (fileInputRef.current) fileInputRef.current.value = "";
    },
    [input, files, isActive, sendMessage],
  );

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(139, 92, 246, 0.08)",
          background:
            "linear-gradient(180deg, rgba(139, 92, 246, 0.02) 0%, transparent 100%)",
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
            style={{ background: "rgba(139, 92, 246, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#8b5cf6" }}
          >
            AI SDK
          </h1>
        </div>
        <div className="flex items-center gap-3">
          <ProviderSelector
            providers={PROVIDERS}
            selected={selectedProvider}
            onChange={setSelectedProvider}
            disabled={isActive}
          />
          <ModelSelector
            models={MODELS_BY_PROVIDER[selectedProvider]}
            selected={selectedModel}
            onChange={setSelectedModel}
            disabled={isActive}
          />
          <button
            onClick={() => setMemoryPanelOpen((v) => !v)}
            className="text-[10px] font-mono font-bold tracking-wider uppercase px-2 py-1 rounded transition-all"
            style={{
              background: memoryPanelOpen
                ? "rgba(139, 92, 246, 0.15)"
                : "rgba(139, 92, 246, 0.05)",
              border: "1px solid rgba(139, 92, 246, 0.15)",
              color: memoryPanelOpen ? "#a78bfa" : "#64748b",
            }}
          >
            Memory
          </button>
        </div>
      </header>

      <MemoryPanel
        open={memoryPanelOpen}
        onClose={() => setMemoryPanelOpen(false)}
      />

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
            {messages.length === 0 && <EmptyState />}

            {messages.map((msg) => (
              <MessageRow key={msg.id} message={msg} />
            ))}

            {isActive && messages.length > 0 && (
              <div className="flex items-center gap-3 px-4 py-3">
                <Spinner />
                <span className="text-xs text-slate-500 animate-pulse font-mono">
                  {status === "submitted"
                    ? "Sending..."
                    : "Streaming response..."}
                </span>
              </div>
            )}

            {error && (
              <div
                className="mx-4 px-4 py-3 rounded-lg text-xs font-mono"
                style={{
                  background: "rgba(239, 68, 68, 0.08)",
                  border: "1px solid rgba(239, 68, 68, 0.2)",
                  color: "#f87171",
                }}
              >
                <span className="font-bold">Error:</span> Something went wrong.
                <button
                  onClick={() => regenerate()}
                  className="ml-3 underline hover:no-underline"
                >
                  Retry
                </button>
              </div>
            )}

            <div ref={messagesEndRef} />
          </div>

          {/* Input bar */}
          <form
            onSubmit={handleSubmit}
            className="px-6 py-4 shrink-0"
            style={{
              borderTop: "1px solid rgba(139, 92, 246, 0.08)",
              background:
                "linear-gradient(0deg, rgba(139, 92, 246, 0.02) 0%, transparent 100%)",
            }}
          >
            <div
              className="flex items-center gap-3 rounded-xl px-4 py-3"
              style={{
                background: "rgba(139, 92, 246, 0.03)",
                border: "1px solid rgba(139, 92, 246, 0.1)",
              }}
            >
              {/* File attachment button */}
              <label
                className="cursor-pointer text-slate-500 hover:text-slate-300 transition-colors shrink-0"
                title="Attach files"
              >
                <svg
                  width="18"
                  height="18"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  strokeLinecap="round"
                  strokeLinejoin="round"
                >
                  <path d="m21.44 11.05-9.19 9.19a6 6 0 0 1-8.49-8.49l8.57-8.57A4 4 0 1 1 18 8.84l-8.59 8.57a2 2 0 0 1-2.83-2.83l8.49-8.48" />
                </svg>
                <input
                  ref={fileInputRef}
                  type="file"
                  multiple
                  className="hidden"
                  onChange={(e) => {
                    if (e.target.files?.length) setFiles(e.target.files);
                  }}
                />
              </label>

              {/* File pills */}
              {files && files.length > 0 && (
                <div className="flex gap-1.5 shrink-0">
                  {Array.from(files).map((f, i) => (
                    <span
                      key={i}
                      className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                      style={{
                        background: "rgba(139, 92, 246, 0.1)",
                        color: "#a78bfa",
                        border: "1px solid rgba(139, 92, 246, 0.2)",
                      }}
                    >
                      {f.name}
                    </span>
                  ))}
                </div>
              )}

              <input
                type="text"
                value={input}
                onChange={(e) => setInput(e.target.value)}
                placeholder="Ask Claude anything..."
                disabled={isActive}
                className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-600 outline-none font-mono"
              />

              {isActive ? (
                <button
                  type="button"
                  onClick={() => stop()}
                  className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-all"
                  style={{
                    background: "rgba(239, 68, 68, 0.15)",
                    border: "1px solid rgba(239, 68, 68, 0.3)",
                    color: "#f87171",
                  }}
                >
                  Stop
                </button>
              ) : (
                <button
                  type="submit"
                  disabled={!input.trim() && !files?.length}
                  className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-all disabled:opacity-30"
                  style={{
                    background: "rgba(139, 92, 246, 0.15)",
                    border: "1px solid rgba(139, 92, 246, 0.3)",
                    color: "#8b5cf6",
                  }}
                >
                  Send
                </button>
              )}
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}

// ── Helpers ──────────────────────────────────────────────────────────

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

// ── Session Sidebar ─────────────────────────────────────────────────

function SessionSidebar({
  sessions,
  activeSessionId,
  loading,
  onSelectSession,
  onNewSession,
  onRenameSession,
  onDeleteSession,
}: {
  sessions: AiSdkSession[];
  activeSessionId: string | null;
  loading: boolean;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onRenameSession: (id: string, title: string) => void;
  onDeleteSession: (id: string) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editTitle, setEditTitle] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const startRename = (id: string, currentTitle: string) => {
    setEditingId(id);
    setEditTitle(currentTitle);
  };

  const commitRename = (id: string) => {
    if (editTitle.trim()) {
      onRenameSession(id, editTitle.trim());
    }
    setEditingId(null);
  };

  return (
    <div
      className="w-60 shrink-0 flex flex-col overflow-hidden"
      style={{
        borderRight: "1px solid rgba(139, 92, 246, 0.06)",
        background: "rgba(0, 0, 0, 0.15)",
      }}
    >
      {/* New session button */}
      <button
        onClick={onNewSession}
        className="m-3 px-3 py-2 rounded-lg text-xs font-bold tracking-wider uppercase transition-all hover:opacity-90"
        style={{
          background: "rgba(139, 92, 246, 0.1)",
          border: "1px solid rgba(139, 92, 246, 0.2)",
          color: "#8b5cf6",
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
                  ? "rgba(139, 92, 246, 0.06)"
                  : "transparent",
                borderLeft: isActive
                  ? "2px solid #8b5cf6"
                  : "2px solid transparent",
              }}
              onClick={() => !isEditing && onSelectSession(s.id)}
            >
              {isEditing ? (
                <input
                  autoFocus
                  value={editTitle}
                  onChange={(e) => setEditTitle(e.target.value)}
                  onBlur={() => commitRename(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename(s.id);
                    if (e.key === "Escape") setEditingId(null);
                  }}
                  onClick={(e) => e.stopPropagation()}
                  className="w-full bg-transparent text-xs text-slate-200 font-mono outline-none px-1 py-0.5 rounded"
                  style={{
                    border: "1px solid rgba(139, 92, 246, 0.3)",
                  }}
                />
              ) : (
                <>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-300 font-mono truncate flex-1">
                      {s.title}
                    </span>
                    {/* Hover actions */}
                    <div className="hidden group-hover:flex items-center gap-1 ml-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          startRename(s.id, s.title);
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
                    <span
                      className="text-[9px] font-mono px-1 rounded"
                      style={{
                        background: "rgba(139, 92, 246, 0.08)",
                        color: "#64748b",
                      }}
                    >
                      {s.model.replace("claude-", "")}
                    </span>
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

// ── Provider Selector ────────────────────────────────────────────────

function ProviderSelector<T extends string>({
  providers,
  selected,
  onChange,
  disabled,
}: {
  providers: readonly { id: T; label: string }[];
  selected: T;
  onChange: (id: T) => void;
  disabled: boolean;
}) {
  return (
    <select
      value={selected}
      onChange={(e) => onChange(e.target.value as T)}
      disabled={disabled}
      className="bg-transparent text-[10px] font-mono text-slate-400 outline-none cursor-pointer px-2 py-1 rounded"
      style={{
        border: "1px solid rgba(139, 92, 246, 0.15)",
        background: "rgba(139, 92, 246, 0.05)",
      }}
    >
      {providers.map((p) => (
        <option key={p.id} value={p.id} className="bg-[#0c1018] text-slate-300">
          {p.label}
        </option>
      ))}
    </select>
  );
}

// ── Model Selector ───────────────────────────────────────────────────

function ModelSelector({
  models,
  selected,
  onChange,
  disabled,
}: {
  models: { id: string; label: string }[];
  selected: string;
  onChange: (id: string) => void;
  disabled: boolean;
}) {
  return (
    <select
      value={selected}
      onChange={(e) => onChange(e.target.value)}
      disabled={disabled}
      className="bg-transparent text-[10px] font-mono text-slate-400 outline-none cursor-pointer px-2 py-1 rounded"
      style={{
        border: "1px solid rgba(139, 92, 246, 0.15)",
        background: "rgba(139, 92, 246, 0.05)",
      }}
    >
      {models.map((m) => (
        <option key={m.id} value={m.id} className="bg-[#0c1018] text-slate-300">
          {m.label}
        </option>
      ))}
    </select>
  );
}

// ── Message Row ──────────────────────────────────────────────────────

function MessageRow({ message }: { message: UIMessage }) {
  const meta = (message as ChatUIMessage).metadata as
    | ChatMetadata
    | undefined;

  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-[70%] rounded-xl px-4 py-3"
          style={{
            background: "rgba(139, 92, 246, 0.08)",
            border: "1px solid rgba(139, 92, 246, 0.15)",
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
            if (
              part.type === "file" &&
              part.mediaType?.startsWith("image/")
            ) {
              return (
                <img
                  key={i}
                  src={part.url}
                  alt={part.filename ?? "attachment"}
                  className="max-w-full rounded-lg mt-2"
                />
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
          border: "1px solid rgba(139, 92, 246, 0.1)",
        }}
      >
        {/* Header */}
        <div className="flex items-center gap-2 mb-2">
          <span
            className="text-[10px] font-bold font-mono tracking-wider"
            style={{ color: "#8b5cf6" }}
          >
            CLAUDE
          </span>
          {meta?.model && (
            <span
              className="text-[9px] font-mono px-1 rounded"
              style={{
                background: "rgba(139, 92, 246, 0.08)",
                color: "#64748b",
              }}
            >
              {meta.model}
            </span>
          )}
          {meta?.totalUsage?.totalTokens != null && (
            <span className="text-[9px] font-mono text-slate-600 ml-auto">
              {meta.totalUsage.totalTokens} tokens
            </span>
          )}
        </div>

        {/* Parts */}
        {message.parts.map((part, i) => {
          if (part.type === "text") {
            return (
              <div
                key={i}
                className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed prose-invert"
              >
                <FormattedText text={part.text} />
              </div>
            );
          }

          if (part.type === "reasoning") {
            return <ReasoningBlock key={i} text={part.text} />;
          }

          if (
            part.type === "file" &&
            part.mediaType?.startsWith("image/")
          ) {
            return (
              <img
                key={i}
                src={part.url}
                alt="Generated"
                className="max-w-full rounded-lg my-2"
              />
            );
          }

          if (part.type === "dynamic-tool") {
            return (
              <ToolCallBlock
                key={`${part.toolCallId}-${i}`}
                toolName={part.toolName}
                toolCallId={part.toolCallId}
                args={(part.input as Record<string, unknown>) ?? {}}
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

          if (part.type === "source-url") {
            return (
              <a
                key={i}
                href={part.url}
                target="_blank"
                rel="noopener noreferrer"
                className="text-xs font-mono underline"
                style={{ color: "#a78bfa" }}
              >
                {part.title ?? new URL(part.url).hostname}
              </a>
            );
          }

          return null;
        })}
      </div>
    </div>
  );
}

// ── Reasoning Block ──────────────────────────────────────────────────

function ReasoningBlock({ text }: { text: string }) {
  const [expanded, setExpanded] = useState(false);

  return (
    <div
      className="my-2 rounded-lg overflow-hidden"
      style={{
        border: "1px solid rgba(139, 92, 246, 0.1)",
        background: "rgba(139, 92, 246, 0.03)",
      }}
    >
      <button
        onClick={() => setExpanded(!expanded)}
        className="w-full flex items-center gap-2 px-3 py-2 text-left"
      >
        <span
          className="text-[10px] font-bold font-mono tracking-wider"
          style={{ color: "#7c3aed" }}
        >
          THINKING
        </span>
        <span className="text-[9px] text-slate-600 font-mono">
          {expanded ? "collapse" : "expand"}
        </span>
        <span className="ml-auto text-slate-600 text-[10px]">
          {expanded ? "\u25B2" : "\u25BC"}
        </span>
      </button>
      {expanded && (
        <div className="px-3 pb-3">
          <pre className="text-xs text-slate-500 font-mono whitespace-pre-wrap leading-relaxed overflow-x-auto">
            {text}
          </pre>
        </div>
      )}
    </div>
  );
}

// ── Formatted Text (basic code block detection) ──────────────────────

function FormattedText({ text }: { text: string }) {
  // Split by code fences
  const parts = text.split(/(```[\s\S]*?```)/g);

  return (
    <>
      {parts.map((part, i) => {
        if (part.startsWith("```") && part.endsWith("```")) {
          const firstNewline = part.indexOf("\n");
          const lang = part.slice(3, firstNewline).trim();
          const code = part.slice(firstNewline + 1, -3);
          return (
            <pre
              key={i}
              className="my-2 p-3 rounded-lg overflow-x-auto text-xs"
              style={{
                background: "rgba(0, 0, 0, 0.3)",
                border: "1px solid rgba(139, 92, 246, 0.08)",
              }}
            >
              {lang && (
                <span
                  className="text-[9px] font-bold tracking-wider block mb-1"
                  style={{ color: "#7c3aed" }}
                >
                  {lang.toUpperCase()}
                </span>
              )}
              <code className="text-slate-300">{code}</code>
            </pre>
          );
        }
        return <span key={i}>{part}</span>;
      })}
    </>
  );
}

// ── Empty State ──────────────────────────────────────────────────────

function EmptyState() {
  return (
    <div className="flex items-center justify-center h-full">
      <div className="text-center max-w-md">
        <div
          className="text-sm font-bold tracking-wider uppercase mb-2"
          style={{ color: "#8b5cf6" }}
        >
          AI SDK Chat
        </div>
        <p className="text-slate-500 text-xs leading-relaxed font-mono mb-4">
          Chat with Claude via Anthropic API or Claude Code CLI.
          Supports streaming, reasoning, attachments, and provider/model selection.
        </p>
        <div className="flex gap-2 justify-center flex-wrap">
          {[
            "Analyze a dataset",
            "Explain an anomaly pattern",
            "Write a SQL query",
          ].map((s) => (
            <span
              key={s}
              className="text-[10px] font-mono px-2 py-1 rounded cursor-default"
              style={{
                background: "rgba(139, 92, 246, 0.06)",
                border: "1px solid rgba(139, 92, 246, 0.1)",
                color: "#a78bfa",
              }}
            >
              {s}
            </span>
          ))}
        </div>
      </div>
    </div>
  );
}

// ── Spinner ──────────────────────────────────────────────────────────

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
