"use client";

import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import type { UIMessage } from "ai";
import type { AthenaSchema } from "@/lib/db/athena-connections";
import { schemaStats } from "@/lib/ai-sdk/tools/athena-schema";

// ── Types ──────────────────────────────────────────────────────────

/** Handle ref for parent to send error feedback to the AI. */
export interface AthenaAIChatHandle {
  sendErrorFeedback: (errorMsg: string, originalSql: string) => void;
}

interface AthenaAIChatProps {
  connectionId: string;
  database?: string;
  schema: AthenaSchema | null;
  onSqlGenerated: (sql: string) => void;
  onExecuteRequest?: () => void;
  /** Ref for parent to call sendErrorFeedback. */
  chatRef?: React.RefObject<AthenaAIChatHandle | null>;
}

// ── SQL extraction from markdown ───────────────────────────────────

const SQL_BLOCK_RE = /```sql\n([\s\S]*?)```/gi;

function extractSql(text: string): string | null {
  const matches = [...text.matchAll(SQL_BLOCK_RE)];
  if (matches.length === 0) return null;
  // Return the last SQL block (most likely the final/corrected query)
  return matches[matches.length - 1][1].trim();
}

// ── Component ──────────────────────────────────────────────────────

export default function AthenaAIChat({
  connectionId,
  database,
  schema,
  onSqlGenerated,
  onExecuteRequest,
  chatRef,
}: AthenaAIChatProps) {
  const bottomRef = useRef<HTMLDivElement>(null);
  const [lastInsertedSql, setLastInsertedSql] = useState<string | null>(null);

  const transport = useMemo(
    () =>
      new DefaultChatTransport({
        api: `/api/athena/${encodeURIComponent(connectionId)}/ai-query`,
        body: { database },
      }),
    [connectionId, database],
  );

  const { messages, sendMessage, status, error } = useChat({
    transport,
  });

  // Auto-scroll on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Extract SQL from the latest assistant message and inject into editor
  useEffect(() => {
    if (status !== "ready") return;
    const lastAssistant = [...messages]
      .reverse()
      .find((m) => m.role === "assistant");
    if (!lastAssistant) return;

    const text = lastAssistant.parts
      .filter((p): p is { type: "text"; text: string } => p.type === "text")
      .map((p) => p.text)
      .join("");

    const sql = extractSql(text);
    if (sql && sql !== lastInsertedSql) {
      onSqlGenerated(sql);
      setLastInsertedSql(sql);
    }
  }, [messages, status, onSqlGenerated, lastInsertedSql]);

  // Send an error message back to the AI for correction
  const sendErrorFeedback = useCallback(
    (errorMsg: string, originalSql: string) => {
      sendMessage({
        role: "user",
        parts: [
          {
            type: "text",
            text: `The query failed with this error:\n\n\`\`\`\n${errorMsg}\n\`\`\`\n\nOriginal SQL:\n\`\`\`sql\n${originalSql}\n\`\`\`\n\nPlease fix the SQL.`,
          },
        ],
      });
    },
    [sendMessage],
  );

  // Expose sendErrorFeedback to parent via chatRef
  useEffect(() => {
    if (chatRef) {
      (chatRef as React.MutableRefObject<AthenaAIChatHandle | null>).current = {
        sendErrorFeedback,
      };
    }
    return () => {
      if (chatRef) {
        (chatRef as React.MutableRefObject<AthenaAIChatHandle | null>).current = null;
      }
    };
  }, [chatRef, sendErrorFeedback]);

  const handleSubmit = useCallback(
    (text: string) => {
      if (!text.trim() || status !== "ready") return;
      sendMessage({
        role: "user",
        parts: [{ type: "text", text }],
      });
    },
    [sendMessage, status],
  );

  const stats = schemaStats(schema);
  const isStreaming = status === "streaming";

  // Suggestions for empty state
  const suggestions = schema
    ? buildSuggestions(schema)
    : ["Refresh the schema first"];

  return (
    <div className="flex flex-col h-full">
      {/* Schema context badge */}
      <div
        className="px-3 py-2 shrink-0 flex items-center gap-2"
        style={{ borderBottom: "1px solid rgba(16, 185, 129, 0.06)" }}
      >
        <span
          className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
          style={{
            background: "rgba(16, 185, 129, 0.08)",
            color: "#10b981",
          }}
        >
          AI Context: {stats.databases} db, {stats.tables} tables
        </span>
        {isStreaming && (
          <span className="text-[9px] text-blue-400 font-mono animate-pulse">
            thinking...
          </span>
        )}
      </div>

      {/* Message list */}
      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3">
        {messages.length === 0 && (
          <EmptyState
            suggestions={suggestions}
            onSuggestionClick={handleSubmit}
          />
        )}

        {messages.map((msg) => (
          <MessageBubble
            key={msg.id}
            message={msg}
            onExecuteRequest={onExecuteRequest}
          />
        ))}

        {error && (
          <div
            className="rounded-lg px-3 py-2 text-xs text-red-400 font-mono"
            style={{
              background: "rgba(255, 71, 87, 0.06)",
              border: "1px solid rgba(255, 71, 87, 0.15)",
            }}
          >
            {error.message}
          </div>
        )}

        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <ChatInput onSubmit={handleSubmit} disabled={isStreaming} />
    </div>
  );
}

// ── Empty state with suggestions ──────────────────────────────────

function EmptyState({
  suggestions,
  onSuggestionClick,
}: {
  suggestions: string[];
  onSuggestionClick: (text: string) => void;
}) {
  return (
    <div className="flex flex-col items-center justify-center py-8">
      <span className="text-slate-600 text-xs font-mono mb-4">
        Ask me to write SQL for you
      </span>
      <div className="flex flex-wrap gap-1.5 justify-center">
        {suggestions.map((s, i) => (
          <button
            key={i}
            onClick={() => onSuggestionClick(s)}
            className="text-[10px] font-medium px-2.5 py-1 rounded-lg transition-all hover:opacity-80"
            style={{
              color: "#10b981",
              background: "rgba(16, 185, 129, 0.06)",
              border: "1px solid rgba(16, 185, 129, 0.12)",
            }}
          >
            {s}
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Message bubble ─────────────────────────────────────────────────

function MessageBubble({
  message,
  onExecuteRequest,
}: {
  message: UIMessage;
  onExecuteRequest?: () => void;
}) {
  const isUser = message.role === "user";

  const text = message.parts
    .filter((p): p is { type: "text"; text: string } => p.type === "text")
    .map((p) => p.text)
    .join("");

  const hasSql = SQL_BLOCK_RE.test(text);
  // Reset lastIndex after test
  SQL_BLOCK_RE.lastIndex = 0;

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className={`max-w-[95%] rounded-xl px-3 py-2 text-xs font-mono leading-relaxed ${
          isUser ? "text-slate-200" : "text-slate-300"
        }`}
        style={{
          background: isUser
            ? "rgba(16, 185, 129, 0.08)"
            : "rgba(6, 8, 13, 0.6)",
          border: `1px solid ${isUser ? "rgba(16, 185, 129, 0.15)" : "rgba(30, 41, 59, 0.4)"}`,
        }}
      >
        <MessageContent text={text} />

        {/* Execute button for SQL-containing messages */}
        {hasSql && !isUser && onExecuteRequest && (
          <div className="mt-2 flex items-center gap-2">
            <button
              onClick={onExecuteRequest}
              className="text-[9px] font-bold uppercase tracking-wider px-2 py-1 rounded transition-all hover:opacity-80"
              style={{
                color: "#10b981",
                background: "rgba(16, 185, 129, 0.1)",
                border: "1px solid rgba(16, 185, 129, 0.3)",
              }}
            >
              Execute in Editor
            </button>
            <span className="text-[9px] text-slate-600">
              SQL inserted into editor
            </span>
          </div>
        )}
      </div>
    </div>
  );
}

// ── Message content with SQL highlighting ──────────────────────────

function MessageContent({ text }: { text: string }) {
  // Split text around SQL blocks for styled rendering
  const parts: { type: "text" | "sql"; content: string }[] = [];
  let lastIndex = 0;
  const regex = /```sql\n([\s\S]*?)```/gi;
  let match;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ type: "text", content: text.slice(lastIndex, match.index) });
    }
    parts.push({ type: "sql", content: match[1].trim() });
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < text.length) {
    parts.push({ type: "text", content: text.slice(lastIndex) });
  }

  return (
    <>
      {parts.map((part, i) =>
        part.type === "sql" ? (
          <pre
            key={i}
            className="my-2 px-3 py-2 rounded-lg text-[10px] overflow-x-auto"
            style={{
              background: "rgba(16, 185, 129, 0.04)",
              border: "1px solid rgba(16, 185, 129, 0.1)",
              color: "#a5f3fc",
            }}
          >
            <code>{part.content}</code>
          </pre>
        ) : (
          <span key={i} className="whitespace-pre-wrap">
            {part.content}
          </span>
        ),
      )}
    </>
  );
}

// ── Chat input ─────────────────────────────────────────────────────

function ChatInput({
  onSubmit,
  disabled,
}: {
  onSubmit: (text: string) => void;
  disabled: boolean;
}) {
  const [input, setInput] = useState("");

  const handleSubmit = (e: React.FormEvent<HTMLFormElement>) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed || disabled) return;
    onSubmit(trimmed);
    setInput("");
  };

  return (
    <form
      onSubmit={handleSubmit}
      className="p-2 shrink-0"
      style={{ borderTop: "1px solid rgba(16, 185, 129, 0.06)" }}
    >
      <div
        className="flex items-center gap-2 rounded-lg px-3 py-2"
        style={{
          background: "rgba(6, 8, 13, 0.6)",
          border: "1px solid rgba(30, 41, 59, 0.6)",
        }}
      >
        <input
          type="text"
          value={input}
          onChange={(e) => setInput(e.target.value)}
          placeholder="Describe the query you need..."
          disabled={disabled}
          className="flex-1 bg-transparent text-xs text-slate-200 placeholder-slate-600 font-mono outline-none disabled:opacity-50"
        />
        <button
          type="submit"
          disabled={!input.trim() || disabled}
          className="text-[9px] font-bold tracking-wider uppercase px-2.5 py-1 rounded-lg transition-all disabled:opacity-20"
          style={{
            color: "#10b981",
            background: "rgba(16, 185, 129, 0.08)",
            border: "1px solid rgba(16, 185, 129, 0.15)",
          }}
        >
          {disabled ? "..." : "Send"}
        </button>
      </div>
    </form>
  );
}

// ── Suggestion builder ─────────────────────────────────────────────

function buildSuggestions(schema: AthenaSchema): string[] {
  const suggestions: string[] = [];
  const firstDb = schema.databases[0];
  if (!firstDb) return ["Show all databases"];

  const firstTable = firstDb.tables[0];
  if (firstTable) {
    suggestions.push(`Show first 10 rows from ${firstDb.name}.${firstTable.name}`);
  }
  suggestions.push("List all tables and their row counts");
  suggestions.push("Show the schema structure");
  if (firstDb.tables.length > 1) {
    suggestions.push(`What columns does ${firstDb.name}.${firstDb.tables[1]?.name ?? firstDb.tables[0]?.name} have?`);
  }
  return suggestions.slice(0, 4);
}

// ── Export sendErrorFeedback type for parent component ──────────────

export type { AthenaAIChatProps };
