"use client";

import { useState, useRef, useEffect, useCallback, useMemo } from "react";
import { useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import type { UIMessage } from "ai";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";
import ToolCallBlock from "@/components/ai-sdk/ToolCallBlock";
import type { ToolCallBlockProps } from "@/components/ai-sdk/ToolCallBlock";

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

// ── Props ──────────────────────────────────────────────────────────────

interface DbAiChatProps {
  db: string;
}

// ── Component ──────────────────────────────────────────────────────────

export default function DbAiChat({ db }: DbAiChatProps) {
  const [input, setInput] = useState("");
  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);

  const transport = useMemo(
    () =>
      new DefaultChatTransport({
        api: "/api/ai-sdk/chat",
        body: {
          model: "claude-sonnet-4-6",
          provider: "anthropic",
          dbConnectionId: db,
        },
      }),
    [db],
  );

  const { messages, sendMessage, status, stop } = useChat({
    transport,
  });

  const isActive = status === "submitted" || status === "streaming";

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Focus input on mount
  useEffect(() => {
    inputRef.current?.focus();
  }, []);

  const handleSubmit = useCallback(
    (e: React.FormEvent<HTMLFormElement>) => {
      e.preventDefault();
      const trimmed = input.trim();
      if (!trimmed || isActive) return;
      sendMessage({ text: trimmed });
      setInput("");
    },
    [input, isActive, sendMessage],
  );

  const handleKeyDown = useCallback(
    (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
      if (e.key === "Enter" && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        const trimmed = input.trim();
        if (!trimmed || isActive) return;
        sendMessage({ text: trimmed });
        setInput("");
      }
    },
    [input, isActive, sendMessage],
  );

  const handleSuggestion = useCallback(
    (text: string) => {
      if (isActive) return;
      sendMessage({ text });
    },
    [isActive, sendMessage],
  );

  const hasMessages = messages.length > 0;

  return (
    <div className="flex flex-col h-[calc(100vh-180px)]">
      {/* ── Messages ────────────────────────────────────────── */}
      <div className="flex-1 overflow-y-auto px-2 py-4 space-y-4">
        {!hasMessages && (
          <EmptyState db={db} onSuggestion={handleSuggestion} />
        )}

        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {isActive && messages.length > 0 && (
          <div className="flex items-center gap-2 px-3 py-2">
            <div
              className="w-2 h-2 rounded-full animate-pulse"
              style={{ background: "#00f0ff" }}
            />
            <span className="text-[10px] font-mono text-slate-500">
              Thinking...
            </span>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* ── Input ───────────────────────────────────────────── */}
      <form
        onSubmit={handleSubmit}
        className="shrink-0 px-2 pb-3 pt-2"
        style={{ borderTop: "1px solid rgba(0, 240, 255, 0.06)" }}
      >
        <div
          className="rounded-lg overflow-hidden"
          style={{
            background: "rgba(6, 8, 13, 0.5)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
          }}
        >
          <textarea
            ref={inputRef}
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={handleKeyDown}
            placeholder="Ask about your data..."
            rows={2}
            className="w-full bg-transparent px-4 py-3 text-sm font-mono text-slate-200 placeholder:text-slate-600 resize-none outline-none"
          />
          <div
            className="flex items-center justify-between px-3 py-2"
            style={{ borderTop: "1px solid rgba(0, 240, 255, 0.04)" }}
          >
            <span className="text-[9px] text-slate-600 font-mono">
              Ctrl+Enter to send
            </span>
            <div className="flex gap-2">
              {isActive && (
                <button
                  type="button"
                  onClick={stop}
                  className="px-3 py-1 text-[10px] font-bold tracking-wider uppercase rounded font-mono transition-all hover:opacity-80"
                  style={{
                    color: "#ff4757",
                    background: "rgba(255, 71, 87, 0.1)",
                    border: "1px solid rgba(255, 71, 87, 0.2)",
                  }}
                >
                  Stop
                </button>
              )}
              <button
                type="submit"
                disabled={isActive || !input.trim()}
                className="px-4 py-1 text-[10px] font-bold tracking-wider uppercase rounded font-mono transition-all hover:opacity-90 disabled:opacity-30"
                style={{
                  color: "#06080d",
                  background: isActive ? "#475569" : "#00f0ff",
                }}
              >
                Send
              </button>
            </div>
          </div>
        </div>
      </form>
    </div>
  );
}

// ── Empty state with suggestions ────────────────────────────────────────

function EmptyState({
  db,
  onSuggestion,
}: {
  db: string;
  onSuggestion: (text: string) => void;
}) {
  const [suggestions, setSuggestions] = useState<string[]>([
    "Show me the 10 largest tables by row count",
    "What columns does each table have?",
    "Find any NULL values in primary key columns",
    "Show me the most recent 20 rows from the first table",
  ]);

  // Load schema-aware suggestions
  useEffect(() => {
    async function loadSuggestions() {
      try {
        const response = await fetch(`/api/db/${encodeURIComponent(db)}/tables`);
        if (!response.ok) return;
        const tables: Array<{ name: string; schema: string; row_count: number }> = await response.json();
        if (tables.length === 0) return;

        // Sort by row count descending, take first table as example
        const sorted = [...tables].sort((a, b) => (b.row_count ?? 0) - (a.row_count ?? 0));
        const firstTable = sorted[0];
        const fullTableName = firstTable.schema === "public"
          ? firstTable.name
          : `${firstTable.schema}.${firstTable.name}`;

        setSuggestions([
          "Show me the 10 largest tables by row count",
          `What are the most recent 20 entries in ${fullTableName}?`,
          "Describe the relationships between tables",
          `Find all rows in ${fullTableName} where any column is NULL`,
        ]);
      } catch (err) {
        console.error("Failed to load schema-aware suggestions:", err);
        // Keep default suggestions on error
      }
    }
    loadSuggestions();
  }, [db]);

  return (
    <div className="flex flex-col items-center justify-center py-12 px-4">
      <div className="text-center mb-8">
        <div
          className="text-lg font-mono font-bold mb-2"
          style={{ color: "#00f0ff" }}
        >
          AI Query Assistant
        </div>
        <p className="text-[11px] font-mono text-slate-500 max-w-md">
          Ask questions about <span style={{ color: "#00f0ff" }}>{decodeURIComponent(db)}</span> in
          natural language. The AI knows your schema and can write + execute SQL for you.
        </p>
      </div>

      <div className="grid grid-cols-2 gap-2 max-w-lg w-full">
        {suggestions.map((s) => (
          <button
            key={s}
            onClick={() => onSuggestion(s)}
            className="text-left px-3 py-2.5 rounded-lg text-[10px] font-mono transition-all hover:opacity-80"
            style={{
              color: "#94a3b8",
              background: "rgba(0, 240, 255, 0.03)",
              border: "1px solid rgba(0, 240, 255, 0.08)",
            }}
            onMouseEnter={(e) => {
              e.currentTarget.style.borderColor = "rgba(0, 240, 255, 0.2)";
              e.currentTarget.style.color = "#e2e8f0";
            }}
            onMouseLeave={(e) => {
              e.currentTarget.style.borderColor = "rgba(0, 240, 255, 0.08)";
              e.currentTarget.style.color = "#94a3b8";
            }}
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Message bubble ──────────────────────────────────────────────────────

function MessageBubble({ message }: { message: UIMessage }) {
  const isUser = message.role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[90%] rounded-xl px-4 py-2.5 text-sm leading-relaxed font-mono ${
          isUser ? "" : ""
        }`}
        style={
          isUser
            ? {
                background: "rgba(0, 240, 255, 0.08)",
                border: "1px solid rgba(0, 240, 255, 0.15)",
                color: "#e2e8f0",
              }
            : {
                background: "rgba(30, 41, 59, 0.3)",
                border: "1px solid rgba(30, 41, 59, 0.4)",
                color: "#cbd5e1",
              }
        }
      >
        {message.parts.map((part, i) => {
          if (part.type === "text") {
            if (isUser) {
              return (
                <span key={i} className="text-sm">
                  {part.text}
                </span>
              );
            }
            return (
              <div key={i} className="prose prose-invert prose-sm max-w-none db-ai-markdown">
                <ReactMarkdown remarkPlugins={[remarkGfm]}>
                  {part.text}
                </ReactMarkdown>
              </div>
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
          return null;
        })}
      </div>
    </div>
  );
}
