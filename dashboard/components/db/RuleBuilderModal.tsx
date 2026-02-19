"use client";

import { useState, useRef, useEffect, useMemo, useCallback } from "react";
import { useChat } from "@ai-sdk/react";
import { DefaultChatTransport } from "ai";
import type { UIMessage } from "ai";
import dynamic from "next/dynamic";
import ToolCallBlock from "@/components/assistant/ToolCallBlock";
import type { ToolCallBlockProps } from "@/components/assistant/ToolCallBlock";

// Lazy-load CodeEditor to avoid SSR issues with CodeMirror
const CodeEditor = dynamic(() => import("@/components/db/CodeEditor"), {
  ssr: false,
  loading: () => (
    <div className="flex items-center justify-center h-full text-slate-600 text-xs font-mono">
      Loading editor...
    </div>
  ),
});

// ── Types ──────────────────────────────────────────────────────────

export interface RuleBuilderModalProps {
  open: boolean;
  onClose: () => void;
  onSaveYaml?: (yaml: string) => void;
  onRuleSaved?: () => void;
}

interface ValidationResult {
  status: "idle" | "loading" | "valid" | "error";
  kind?: string;
  id?: string;
  name?: string;
  errors?: string[];
}

// ── Tool invocation state mapping ──────────────────────────────────

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

// ── YAML block extraction ──────────────────────────────────────────

const YAML_BLOCK_RE = /```ya?ml\n([\s\S]*?)```/gi;

function extractYaml(text: string): string | null {
  const matches = [...text.matchAll(YAML_BLOCK_RE)];
  if (matches.length === 0) return null;
  return matches[matches.length - 1][1].trim();
}

// ── Component ──────────────────────────────────────────────────────

export default function RuleBuilderModal(props: RuleBuilderModalProps) {
  const { open, onClose, onSaveYaml } = props;
  const bottomRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);
  const [sessionId] = useState(() => crypto.randomUUID());
  const [input, setInput] = useState("");
  const [validation, setValidation] = useState<ValidationResult>({ status: "idle" });

  const transport = useMemo(
    () =>
      new DefaultChatTransport({
        api: "/api/rules/chat",
        headers: { "X-Session-Id": sessionId },
      }),
    [sessionId],
  );

  const { messages, sendMessage, status, error } = useChat({ transport });

  const isStreaming = status === "streaming";
  const isSubmitted = status === "submitted";
  const isBusy = isStreaming || isSubmitted;

  // Auto-scroll on new messages
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Focus input when modal opens
  useEffect(() => {
    if (open) {
      setTimeout(() => inputRef.current?.focus(), 100);
    }
  }, [open]);

  // Close on Escape
  useEffect(() => {
    if (!open) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  // ── Submit handler ──────────────────────────────────────────────

  const handleSubmit = useCallback(
    (e: { preventDefault: () => void }) => {
      e.preventDefault();
      const text = input.trim();
      if (!text || isBusy) return;
      sendMessage({ role: "user", parts: [{ type: "text", text }] });
      setInput("");
    },
    [input, isBusy, sendMessage],
  );

  // ── Extract latest YAML from all assistant messages ─────────────

  const latestYaml = useMemo(() => {
    for (let i = messages.length - 1; i >= 0; i--) {
      const msg = messages[i];
      if (msg.role === "assistant") {
        const text = msg.parts
          .filter((p): p is { type: "text"; text: string } => p.type === "text")
          .map((p) => p.text)
          .join("");
        const yaml = extractYaml(text);
        if (yaml) return yaml;
      }
    }
    return null;
  }, [messages]);

  // ── Validate YAML when it changes ──────────────────────────────

  useEffect(() => {
    if (!latestYaml) {
      setValidation({ status: "idle" });
      return;
    }

    let cancelled = false;
    setValidation({ status: "loading" });

    fetch("/api/rules/validate", {
      method: "POST",
      headers: { "Content-Type": "application/yaml" },
      body: latestYaml,
    })
      .then(async (res) => {
        if (cancelled) return;
        const json = await res.json();
        if (cancelled) return;

        if (res.ok && json.valid) {
          setValidation({
            status: "valid",
            kind: json.kind,
            id: json.id,
            name: json.name,
          });
        } else {
          setValidation({
            status: "error",
            errors: json.errors || ["Unknown validation error"],
          });
        }
      })
      .catch(() => {
        if (!cancelled) {
          setValidation({
            status: "error",
            errors: ["Failed to reach validation endpoint"],
          });
        }
      });

    return () => { cancelled = true; };
  }, [latestYaml]);

  const handleEditInEditor = useCallback(() => {
    if (latestYaml && onSaveYaml) {
      onSaveYaml(latestYaml);
      onClose();
    }
  }, [latestYaml, onSaveYaml, onClose]);

  // ── Suggestions ────────────────────────────────────────────────

  const suggestions = [
    "Create an anomaly rule for high-value transfers",
    "Build a pattern rule for login brute-force",
    "Create an entity schema for user accounts",
    "Help me write a feature extraction config",
  ];

  const handleSuggestion = useCallback(
    (text: string) => {
      if (isBusy) return;
      sendMessage({ role: "user", parts: [{ type: "text", text }] });
    },
    [isBusy, sendMessage],
  );

  if (!open) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
    >
      <div
        className="flex flex-col w-full max-w-6xl rounded-2xl overflow-hidden"
        style={{
          height: "min(90vh, 800px)",
          background: "linear-gradient(180deg, #0c1018 0%, #080b12 100%)",
          border: "1px solid rgba(249, 115, 22, 0.12)",
          boxShadow: "0 0 60px rgba(249, 115, 22, 0.06), 0 25px 50px rgba(0, 0, 0, 0.5)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* ── Header ─────────────────────────────────────────────── */}
        <div
          className="px-5 py-3 flex items-center justify-between shrink-0"
          style={{
            borderBottom: "1px solid rgba(249, 115, 22, 0.08)",
            background: "linear-gradient(180deg, rgba(249, 115, 22, 0.03) 0%, transparent 100%)",
          }}
        >
          <div className="flex items-center gap-3">
            <span
              className="text-sm font-bold tracking-wider"
              style={{ color: "#f97316" }}
            >
              Rule Builder
            </span>
            <span
              className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(249, 115, 22, 0.08)",
                color: "#f97316",
                border: "1px solid rgba(249, 115, 22, 0.15)",
              }}
            >
              AI Chat
            </span>
          </div>

          <div className="flex items-center gap-2">
            {/* Edit in Editor button — appears when YAML is available */}
            {latestYaml && onSaveYaml && (
              <button
                onClick={handleEditInEditor}
                className="text-[10px] font-bold uppercase tracking-wider px-3 py-1.5 rounded-lg transition-all hover:opacity-80"
                style={{
                  background: "rgba(249, 115, 22, 0.1)",
                  border: "1px solid rgba(249, 115, 22, 0.3)",
                  color: "#f97316",
                }}
              >
                Edit in Editor
              </button>
            )}
            <button
              onClick={onClose}
              className="text-slate-500 hover:text-slate-300 transition-colors text-lg leading-none px-1"
              title="Close (Esc)"
            >
              &times;
            </button>
          </div>
        </div>

        {/* ── Split Panel: Chat + YAML Preview ─────────────────── */}
        <div className="flex-1 flex flex-col lg:flex-row min-h-0">
          {/* ── Left: Chat Panel ───────────────────────────────── */}
          <div className="flex-1 flex flex-col min-h-0 min-w-0">
            {/* Messages */}
            <div className="flex-1 overflow-y-auto px-5 py-4 space-y-4">
              {/* Empty state */}
              {messages.length === 0 && (
                <div className="flex flex-col items-center justify-center h-full">
                  <div className="text-center max-w-md">
                    <div
                      className="text-sm font-bold tracking-wider uppercase mb-2"
                      style={{ color: "#f97316" }}
                    >
                      AI Rule Builder
                    </div>
                    <p className="text-slate-500 text-xs leading-relaxed font-mono mb-6">
                      Describe the rule you want to create. The AI will help you build
                      valid YAML, validate it, and optionally dry-run against your data.
                    </p>

                    {/* Suggestion chips */}
                    <div className="flex flex-wrap gap-2 justify-center">
                      {suggestions.map((s, i) => (
                        <button
                          key={i}
                          onClick={() => handleSuggestion(s)}
                          className="text-[10px] font-medium px-3 py-1.5 rounded-lg transition-all hover:opacity-80"
                          style={{
                            color: "#f97316",
                            background: "rgba(249, 115, 22, 0.06)",
                            border: "1px solid rgba(249, 115, 22, 0.12)",
                          }}
                        >
                          {s}
                        </button>
                      ))}
                    </div>
                  </div>
                </div>
              )}

              {/* Message list */}
              {messages.map((msg) => (
                <RuleMessageRow key={msg.id} message={msg} />
              ))}

              {/* Streaming indicator */}
              {isBusy && messages.length > 0 && (
                <div className="flex items-center gap-3 px-4 py-3">
                  <Spinner />
                  <span className="text-xs text-slate-500 animate-pulse font-mono">
                    {isSubmitted ? "Sending..." : "Building rule..."}
                  </span>
                </div>
              )}

              {/* Error display */}
              {error && (
                <div
                  className="rounded-lg px-4 py-2.5 text-xs text-red-400 font-mono"
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

            {/* ── Input bar ────────────────────────────────────── */}
            <form
              onSubmit={handleSubmit}
              className="px-5 py-3 shrink-0"
              style={{
                borderTop: "1px solid rgba(249, 115, 22, 0.08)",
                background: "linear-gradient(0deg, rgba(249, 115, 22, 0.02) 0%, transparent 100%)",
              }}
            >
              <div
                className="flex items-center gap-3 rounded-xl px-4 py-3"
                style={{
                  background: "rgba(249, 115, 22, 0.03)",
                  border: "1px solid rgba(249, 115, 22, 0.1)",
                }}
              >
                <input
                  ref={inputRef}
                  type="text"
                  value={input}
                  onChange={(e) => setInput(e.target.value)}
                  placeholder="Describe the rule you want to create..."
                  disabled={isBusy}
                  className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-600 outline-none font-mono"
                />
                <button
                  type="submit"
                  disabled={isBusy || !input.trim()}
                  className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-all disabled:opacity-30"
                  style={{
                    background: "rgba(249, 115, 22, 0.15)",
                    border: "1px solid rgba(249, 115, 22, 0.3)",
                    color: "#f97316",
                  }}
                >
                  {isBusy ? "Running..." : "Send"}
                </button>
              </div>
            </form>
          </div>

          {/* ── Right: YAML Preview Panel ──────────────────────── */}
          <div
            className="lg:w-[420px] shrink-0 flex flex-col min-h-0 border-t lg:border-t-0 lg:border-l"
            style={{ borderColor: "rgba(249, 115, 22, 0.08)" }}
          >
            {/* Preview header with validation badge */}
            <div
              className="px-4 py-2.5 flex items-center justify-between shrink-0"
              style={{
                borderBottom: "1px solid rgba(249, 115, 22, 0.06)",
                background: "rgba(249, 115, 22, 0.015)",
              }}
            >
              <span
                className="text-[10px] font-bold font-mono uppercase tracking-wider"
                style={{ color: "rgba(249, 115, 22, 0.6)" }}
              >
                YAML Preview
              </span>
              <ValidationBadge validation={validation} />
            </div>

            {/* Editor or empty state */}
            <div className="flex-1 min-h-0 overflow-hidden">
              {latestYaml ? (
                <CodeEditor
                  value={latestYaml}
                  language="yaml"
                  readOnly
                  minHeight="100%"
                  maxHeight="100%"
                  className="h-full [&_.cm-editor]:!h-full [&_.cm-editor]:!rounded-none [&_.cm-editor]:!border-0"
                />
              ) : (
                <div className="flex flex-col items-center justify-center h-full px-6">
                  <div
                    className="w-10 h-10 rounded-xl flex items-center justify-center mb-3"
                    style={{
                      background: "rgba(249, 115, 22, 0.06)",
                      border: "1px solid rgba(249, 115, 22, 0.1)",
                    }}
                  >
                    <span className="text-lg" style={{ color: "rgba(249, 115, 22, 0.3)" }}>
                      {"{ }"}
                    </span>
                  </div>
                  <p className="text-[11px] text-slate-600 font-mono text-center leading-relaxed">
                    YAML will appear here as the AI generates your rule definition.
                  </p>
                </div>
              )}
            </div>

            {/* Validation details (errors) */}
            {validation.status === "error" && validation.errors && (
              <div
                className="px-4 py-3 shrink-0 overflow-y-auto max-h-32"
                style={{
                  borderTop: "1px solid rgba(255, 71, 87, 0.15)",
                  background: "rgba(255, 71, 87, 0.03)",
                }}
              >
                {validation.errors.map((err, i) => (
                  <p key={i} className="text-[10px] text-red-400 font-mono leading-relaxed">
                    {err}
                  </p>
                ))}
              </div>
            )}

            {/* Valid details */}
            {validation.status === "valid" && validation.kind && (
              <div
                className="px-4 py-2 shrink-0"
                style={{
                  borderTop: "1px solid rgba(249, 115, 22, 0.08)",
                  background: "rgba(249, 115, 22, 0.015)",
                }}
              >
                <div className="flex items-center gap-2 text-[10px] font-mono text-slate-500">
                  <span
                    className="px-1.5 py-0.5 rounded"
                    style={{
                      background: "rgba(249, 115, 22, 0.08)",
                      color: "#f97316",
                      border: "1px solid rgba(249, 115, 22, 0.15)",
                    }}
                  >
                    {validation.kind}
                  </span>
                  {validation.id && (
                    <span className="text-slate-600 truncate">{validation.id}</span>
                  )}
                </div>
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  );
}

// ── Validation Badge ──────────────────────────────────────────────

function ValidationBadge({ validation }: { validation: ValidationResult }) {
  if (validation.status === "idle") return null;

  if (validation.status === "loading") {
    return (
      <div className="flex items-center gap-1.5">
        <div
          className="w-3 h-3 rounded-full animate-spin"
          style={{
            border: "1.5px solid rgba(249, 115, 22, 0.15)",
            borderTopColor: "#f97316",
          }}
        />
        <span className="text-[9px] font-mono text-slate-500">Validating...</span>
      </div>
    );
  }

  if (validation.status === "valid") {
    return (
      <div
        className="flex items-center gap-1.5 px-2 py-0.5 rounded"
        style={{
          background: "rgba(249, 115, 22, 0.08)",
          border: "1px solid rgba(249, 115, 22, 0.2)",
        }}
      >
        <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path
            d="M2 5.5L4 7.5L8 3"
            stroke="#f97316"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
        <span className="text-[9px] font-mono font-bold" style={{ color: "#f97316" }}>
          Valid
        </span>
      </div>
    );
  }

  // error
  return (
    <div
      className="flex items-center gap-1.5 px-2 py-0.5 rounded"
      style={{
        background: "rgba(255, 71, 87, 0.08)",
        border: "1px solid rgba(255, 71, 87, 0.2)",
      }}
    >
      <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
        <path
          d="M3 3L7 7M7 3L3 7"
          stroke="#ff4757"
          strokeWidth="1.5"
          strokeLinecap="round"
        />
      </svg>
      <span className="text-[9px] font-mono font-bold" style={{ color: "#ff4757" }}>
        Invalid
      </span>
    </div>
  );
}

// ── Message Row ────────────────────────────────────────────────────

function RuleMessageRow({ message }: { message: UIMessage }) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-[70%] rounded-xl px-4 py-3"
          style={{
            background: "rgba(249, 115, 22, 0.08)",
            border: "1px solid rgba(249, 115, 22, 0.15)",
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
          border: "1px solid rgba(249, 115, 22, 0.1)",
        }}
      >
        <div className="flex items-center gap-2 mb-2">
          <span
            className="text-[10px] font-bold font-mono tracking-wider"
            style={{ color: "#f97316" }}
          >
            RULE BUILDER
          </span>
        </div>

        {message.parts.map((part, i) => {
          if (part.type === "text") {
            return (
              <RuleMessageContent key={i} text={part.text} />
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
                    "linear-gradient(90deg, transparent, rgba(249, 115, 22, 0.15), transparent)",
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

// ── Message content with YAML highlighting ─────────────────────────

function RuleMessageContent({ text }: { text: string }) {
  const parts: { type: "text" | "yaml"; content: string }[] = [];
  let lastIndex = 0;
  const regex = /```ya?ml\n([\s\S]*?)```/gi;
  let match;

  while ((match = regex.exec(text)) !== null) {
    if (match.index > lastIndex) {
      parts.push({ type: "text", content: text.slice(lastIndex, match.index) });
    }
    parts.push({ type: "yaml", content: match[1].trim() });
    lastIndex = match.index + match[0].length;
  }

  if (lastIndex < text.length) {
    parts.push({ type: "text", content: text.slice(lastIndex) });
  }

  return (
    <>
      {parts.map((part, i) =>
        part.type === "yaml" ? (
          <pre
            key={i}
            className="my-2 px-3 py-2 rounded-lg text-[11px] overflow-x-auto font-mono leading-relaxed"
            style={{
              background: "rgba(249, 115, 22, 0.04)",
              border: "1px solid rgba(249, 115, 22, 0.1)",
              color: "#fbbf24",
            }}
          >
            <code>{part.content}</code>
          </pre>
        ) : (
          <span
            key={i}
            className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed"
          >
            {part.content}
          </span>
        ),
      )}
    </>
  );
}

// ── Spinner ────────────────────────────────────────────────────────

function Spinner() {
  return (
    <div
      className="w-4 h-4 rounded-full animate-spin"
      style={{
        border: "2px solid rgba(249, 115, 22, 0.1)",
        borderTopColor: "#f97316",
      }}
    />
  );
}
