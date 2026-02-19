"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import ReactMarkdown from "react-markdown";
import remarkGfm from "remark-gfm";

// ── Types ──────────────────────────────────────────────────────────

interface ChatMessage {
  role: "user" | "assistant";
  content: string;
}

interface AgentConfig {
  name: string;
  description: string;
  system_prompt: string;
  model: string;
  template_id: string | null;
  skills_config: unknown[];
  mcp_servers_config: unknown[];
  tools_config: unknown[];
}

// ── Styling ────────────────────────────────────────────────────────

const CYAN = "#00f0ff";
const GREEN = "#06d6a0";

const cardStyle: React.CSSProperties = {
  background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
  border: "1px solid rgba(0, 240, 255, 0.1)",
  borderRadius: 12,
};

const inputStyle: React.CSSProperties = {
  background: "rgba(15, 23, 42, 0.8)",
  border: "1px solid rgba(0, 240, 255, 0.15)",
  borderRadius: 8,
  color: "#e2e8f0",
  padding: "10px 14px",
  fontSize: 13,
  fontFamily: "monospace",
  width: "100%",
  outline: "none",
};

// ── Helpers ────────────────────────────────────────────────────────

/** Extract the last JSON code block from markdown text. */
function extractAgentConfig(text: string): AgentConfig | null {
  const jsonBlocks = [...text.matchAll(/```json\s*\n([\s\S]*?)```/g)];
  if (jsonBlocks.length === 0) return null;
  const lastBlock = jsonBlocks[jsonBlocks.length - 1][1];
  try {
    const parsed = JSON.parse(lastBlock);
    if (parsed.name && parsed.system_prompt) return parsed as AgentConfig;
    return null;
  } catch {
    return null;
  }
}

// ── Component ──────────────────────────────────────────────────────

interface AgentCreatorChatProps {
  onSave: (config: AgentConfig) => void;
  onClose: () => void;
}

export default function AgentCreatorChat({
  onSave,
  onClose,
}: AgentCreatorChatProps) {
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [streaming, setStreaming] = useState(false);
  const [streamOutput, setStreamOutput] = useState("");
  const [saving, setSaving] = useState(false);
  const chatEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Extract config from the latest assistant message
  const latestAssistantMsg = [...messages]
    .reverse()
    .find((m) => m.role === "assistant");
  const extractedConfig = latestAssistantMsg
    ? extractAgentConfig(latestAssistantMsg.content)
    : streaming
      ? extractAgentConfig(streamOutput)
      : null;

  // Auto-scroll
  useEffect(() => {
    chatEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages, streamOutput]);

  // Focus input
  useEffect(() => {
    if (!streaming) inputRef.current?.focus();
  }, [streaming]);

  const sendMessage = useCallback(
    async (userMsg: string) => {
      if (!userMsg.trim() || streaming) return;

      const newMessages: ChatMessage[] = [
        ...messages,
        { role: "user", content: userMsg },
      ];
      setMessages(newMessages);
      setInput("");
      setStreaming(true);
      setStreamOutput("");

      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      try {
        const res = await fetch("/api/stille-post/agents/generate", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ messages: newMessages }),
          signal: controller.signal,
        });

        if (!res.ok) {
          const errBody = await res
            .json()
            .catch(() => ({ error: res.statusText }));
          setMessages((prev) => [
            ...prev,
            {
              role: "assistant",
              content: `Error: ${errBody.error || res.statusText}`,
            },
          ]);
          setStreaming(false);
          return;
        }

        const reader = res.body?.getReader();
        if (!reader) {
          setStreaming(false);
          return;
        }

        const decoder = new TextDecoder();
        let buffer = "";
        let accumulated = "";

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";

          let currentEvent = "";
          for (const line of lines) {
            if (line.startsWith("event: ")) {
              currentEvent = line.slice(7).trim();
            } else if (line.startsWith("data: ") && currentEvent) {
              try {
                const data = JSON.parse(line.slice(6));
                if (currentEvent === "agent_token" && data.token) {
                  accumulated += data.token;
                  setStreamOutput(accumulated);
                }
                if (currentEvent === "agent_error") {
                  accumulated += `\n\nError: ${data.error}`;
                  setStreamOutput(accumulated);
                }
              } catch {
                // skip
              }
              currentEvent = "";
            }
          }
        }

        // Finalize
        setMessages((prev) => [
          ...prev,
          { role: "assistant", content: accumulated },
        ]);
        setStreamOutput("");
      } catch (err) {
        if ((err as Error).name !== "AbortError") {
          setMessages((prev) => [
            ...prev,
            {
              role: "assistant",
              content: `Connection error: ${err instanceof Error ? err.message : "Unknown"}`,
            },
          ]);
        }
      } finally {
        setStreaming(false);
      }
    },
    [messages, streaming],
  );

  async function handleSave() {
    if (!extractedConfig) return;
    setSaving(true);
    try {
      onSave(extractedConfig);
    } finally {
      setSaving(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === "Enter" && !e.shiftKey) {
      e.preventDefault();
      sendMessage(input);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex"
      style={{ background: "rgba(0, 0, 0, 0.8)", backdropFilter: "blur(6px)" }}
    >
      {/* Main chat area */}
      <div className="flex-1 flex flex-col max-w-3xl mx-auto py-6 px-4">
        {/* Header */}
        <div className="flex items-center justify-between mb-4 shrink-0">
          <div className="flex items-center gap-3">
            <button
              onClick={onClose}
              className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
            >
              &larr; Back
            </button>
            <div
              className="w-[1px] h-4"
              style={{ background: "rgba(0, 240, 255, 0.12)" }}
            />
            <h2
              className="text-sm font-bold uppercase tracking-wider"
              style={{ color: GREEN }}
            >
              AI Agent Creator
            </h2>
          </div>
          {extractedConfig && (
            <button
              onClick={handleSave}
              disabled={saving}
              className="px-4 py-2 text-xs font-bold uppercase tracking-wider rounded transition-colors"
              style={{
                background: `linear-gradient(135deg, ${GREEN}22, ${CYAN}22)`,
                border: `1px solid ${GREEN}66`,
                color: GREEN,
                opacity: saving ? 0.5 : 1,
              }}
            >
              {saving ? "Saving..." : `Save "${extractedConfig.name}"`}
            </button>
          )}
        </div>

        {/* Chat messages */}
        <div
          className="flex-1 overflow-y-auto space-y-4 mb-4"
          style={{
            ...cardStyle,
            padding: 16,
            minHeight: 0,
          }}
        >
          {/* Welcome message */}
          {messages.length === 0 && !streaming && (
            <div className="text-center py-12">
              <div
                className="text-[10px] font-bold uppercase tracking-[0.15em] mb-3"
                style={{ color: GREEN }}
              >
                AI Agent Creator
              </div>
              <p className="text-sm text-slate-400 font-mono max-w-md mx-auto mb-6">
                Describe the kind of agent you want to create. I&apos;ll
                generate a complete configuration including the system prompt,
                model selection, and tools.
              </p>
              <div className="flex flex-wrap justify-center gap-2">
                {[
                  "Security analyst that monitors login anomalies",
                  "Daily KPI reporter for revenue metrics",
                  "Data quality auditor for ingestion pipelines",
                ].map((suggestion) => (
                  <button
                    key={suggestion}
                    onClick={() => sendMessage(suggestion)}
                    className="text-xs font-mono px-3 py-2 rounded-lg transition-colors"
                    style={{
                      background: "rgba(0, 240, 255, 0.05)",
                      border: "1px solid rgba(0, 240, 255, 0.12)",
                      color: "#94a3b8",
                    }}
                  >
                    {suggestion}
                  </button>
                ))}
              </div>
            </div>
          )}

          {/* Messages */}
          {messages.map((msg, i) => (
            <div
              key={i}
              className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}
            >
              <div
                className="max-w-[85%] rounded-xl px-4 py-3"
                style={
                  msg.role === "user"
                    ? {
                        background: "rgba(0, 240, 255, 0.08)",
                        border: "1px solid rgba(0, 240, 255, 0.15)",
                      }
                    : {
                        background: "rgba(6, 214, 160, 0.05)",
                        border: "1px solid rgba(6, 214, 160, 0.1)",
                      }
                }
              >
                <div
                  className="text-[10px] font-bold uppercase tracking-wider mb-1"
                  style={{ color: msg.role === "user" ? CYAN : GREEN }}
                >
                  {msg.role === "user" ? "You" : "Agent Creator"}
                </div>
                <div className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed">
                  <FormattedMessage content={msg.content} />
                </div>
              </div>
            </div>
          ))}

          {/* Streaming output */}
          {streaming && streamOutput && (
            <div className="flex justify-start">
              <div
                className="max-w-[85%] rounded-xl px-4 py-3"
                style={{
                  background: "rgba(6, 214, 160, 0.05)",
                  border: "1px solid rgba(6, 214, 160, 0.1)",
                }}
              >
                <div
                  className="text-[10px] font-bold uppercase tracking-wider mb-1"
                  style={{ color: GREEN }}
                >
                  Agent Creator
                </div>
                <div className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed">
                  <FormattedMessage content={streamOutput} />
                  <span
                    className="inline-block w-2 h-4 ml-0.5 animate-pulse"
                    style={{ background: GREEN }}
                  />
                </div>
              </div>
            </div>
          )}

          {/* Streaming indicator */}
          {streaming && !streamOutput && (
            <div className="flex justify-start">
              <div
                className="rounded-xl px-4 py-3"
                style={{
                  background: "rgba(6, 214, 160, 0.05)",
                  border: "1px solid rgba(6, 214, 160, 0.1)",
                }}
              >
                <div className="flex items-center gap-2">
                  <div
                    className="w-2 h-2 rounded-full animate-pulse"
                    style={{ background: GREEN }}
                  />
                  <span className="text-xs text-slate-500 font-mono">
                    Generating agent config...
                  </span>
                </div>
              </div>
            </div>
          )}

          <div ref={chatEndRef} />
        </div>

        {/* Input area */}
        <div className="shrink-0">
          <div className="relative">
            <textarea
              ref={inputRef}
              value={input}
              onChange={(e) => setInput(e.target.value)}
              onKeyDown={handleKeyDown}
              placeholder={
                messages.length === 0
                  ? "Describe the agent you want to create..."
                  : "Ask for changes or describe a new agent..."
              }
              rows={2}
              disabled={streaming}
              style={{
                ...inputStyle,
                paddingRight: 60,
                resize: "none",
                opacity: streaming ? 0.5 : 1,
              }}
            />
            <button
              onClick={() => sendMessage(input)}
              disabled={!input.trim() || streaming}
              className="absolute right-2 bottom-2 px-3 py-1.5 text-xs font-bold uppercase tracking-wider rounded transition-colors"
              style={{
                background:
                  input.trim() && !streaming
                    ? `linear-gradient(135deg, ${CYAN}33, ${GREEN}33)`
                    : "transparent",
                border: `1px solid ${input.trim() && !streaming ? `${CYAN}44` : "rgba(100,116,139,0.2)"}`,
                color: input.trim() && !streaming ? CYAN : "#475569",
              }}
            >
              Send
            </button>
          </div>
          <div className="text-[10px] text-slate-600 font-mono mt-1.5 px-1">
            Enter to send, Shift+Enter for new line
          </div>
        </div>
      </div>

      {/* Config preview sidebar */}
      {extractedConfig && (
        <div
          className="w-96 border-l overflow-y-auto p-4 shrink-0"
          style={{
            borderColor: "rgba(0, 240, 255, 0.08)",
            background: "rgba(6, 8, 13, 0.95)",
          }}
        >
          <div
            className="text-[10px] font-bold uppercase tracking-[0.15em] mb-3"
            style={{ color: CYAN }}
          >
            Generated Config
          </div>
          <ConfigPreview config={extractedConfig} />
        </div>
      )}
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────

function FormattedMessage({ content }: { content: string }) {
  return (
    <div className="prose prose-invert prose-sm max-w-none [&_p]:my-1 [&_ul]:my-1 [&_ol]:my-1 [&_li]:my-0.5 [&_h1]:text-base [&_h2]:text-sm [&_h3]:text-xs [&_h1]:text-slate-200 [&_h2]:text-slate-200 [&_h3]:text-slate-300 [&_strong]:text-slate-200 [&_a]:text-cyan-400">
      <ReactMarkdown
        remarkPlugins={[remarkGfm]}
        components={{
          code({ className, children, ...props }) {
            const isBlock = className?.includes("language-");
            if (isBlock) {
              return (
                <pre
                  className="rounded-lg p-3 my-2 text-[11px] overflow-x-auto"
                  style={{
                    background: "rgba(0, 0, 0, 0.3)",
                    border: "1px solid rgba(0, 240, 255, 0.08)",
                    color: "#a5f3fc",
                  }}
                >
                  <code>{children}</code>
                </pre>
              );
            }
            return (
              <code
                className="text-[11px] px-1 py-0.5 rounded"
                style={{
                  background: "rgba(0, 240, 255, 0.08)",
                  color: "#a5f3fc",
                }}
                {...props}
              >
                {children}
              </code>
            );
          },
          pre({ children }) {
            // Let the code component handle pre rendering
            return <>{children}</>;
          },
        }}
      >
        {content}
      </ReactMarkdown>
    </div>
  );
}

function ConfigPreview({ config }: { config: AgentConfig }) {
  const fieldStyle: React.CSSProperties = {
    background: "rgba(15, 23, 42, 0.6)",
    border: "1px solid rgba(0, 240, 255, 0.08)",
    borderRadius: 8,
    padding: "8px 12px",
  };

  return (
    <div className="space-y-3">
      <div style={fieldStyle}>
        <div
          className="text-[9px] font-bold uppercase tracking-wider mb-1"
          style={{ color: "#475569" }}
        >
          Name
        </div>
        <div className="text-sm text-slate-200 font-mono">{config.name}</div>
      </div>

      <div style={fieldStyle}>
        <div
          className="text-[9px] font-bold uppercase tracking-wider mb-1"
          style={{ color: "#475569" }}
        >
          Description
        </div>
        <div className="text-xs text-slate-400 font-mono">
          {config.description}
        </div>
      </div>

      <div style={fieldStyle}>
        <div
          className="text-[9px] font-bold uppercase tracking-wider mb-1"
          style={{ color: "#475569" }}
        >
          Model
        </div>
        <span
          className="text-[11px] font-mono px-2 py-0.5 rounded"
          style={{ background: "rgba(0, 240, 255, 0.08)", color: CYAN }}
        >
          {config.model}
        </span>
      </div>

      {config.template_id && (
        <div style={fieldStyle}>
          <div
            className="text-[9px] font-bold uppercase tracking-wider mb-1"
            style={{ color: "#475569" }}
          >
            Template
          </div>
          <div className="text-xs text-slate-400 font-mono">
            {config.template_id}
          </div>
        </div>
      )}

      <div style={fieldStyle}>
        <div
          className="text-[9px] font-bold uppercase tracking-wider mb-1"
          style={{ color: "#475569" }}
        >
          System Prompt
        </div>
        <div className="text-[11px] text-slate-400 font-mono whitespace-pre-wrap max-h-48 overflow-y-auto leading-relaxed">
          {config.system_prompt}
        </div>
      </div>

      {config.tools_config.length > 0 && (
        <div style={fieldStyle}>
          <div
            className="text-[9px] font-bold uppercase tracking-wider mb-1"
            style={{ color: "#475569" }}
          >
            Tools ({config.tools_config.length})
          </div>
          <div className="text-[11px] text-slate-400 font-mono">
            {config.tools_config.map((t, i) => (
              <div key={i} className="py-0.5">
                {(t as { name?: string }).name || JSON.stringify(t)}
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}
