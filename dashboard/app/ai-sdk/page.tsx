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
  const fileInputRef = useRef<HTMLInputElement>(null);
  const messagesEndRef = useRef<HTMLDivElement>(null);

  // Update model when provider changes
  useEffect(() => {
    const models = MODELS_BY_PROVIDER[selectedProvider];
    setSelectedModel(models[0].id);
  }, [selectedProvider]);

  const transport = useMemo(
    () =>
      new DefaultChatTransport({
        api: "/api/ai-sdk/chat",
        body: { model: selectedModel, provider: selectedProvider },
      }),
    [selectedModel, selectedProvider],
  );

  const {
    messages,
    sendMessage,
    status,
    stop,
    regenerate,
    error,
  } = useChat<ChatUIMessage>({ transport });

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
        </div>
      </header>

      {/* Chat area */}
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
