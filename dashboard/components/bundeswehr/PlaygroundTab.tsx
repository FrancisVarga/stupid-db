"use client";

import { useState, useRef, useEffect, useCallback } from "react";
import { executeAgent, createAgent } from "@/lib/api";
import { Light as SyntaxHighlighter } from "react-syntax-highlighter";
import yamlLang from "react-syntax-highlighter/dist/esm/languages/hljs/yaml";
import atomOneDark from "react-syntax-highlighter/dist/esm/styles/hljs/atom-one-dark";
import YAML from "yaml";

SyntaxHighlighter.registerLanguage("yaml", yamlLang);

// ── Types ──────────────────────────────────────────────────────

type Mode = "agent" | "skill";

interface ChatMessage {
  id: string;
  role: "user" | "assistant" | "error";
  content: string;
  timestamp: number;
}

// ── Helpers ────────────────────────────────────────────────────

function extractYaml(text: string): string | null {
  const match = text.match(/```ya?ml\n([\s\S]*?)\n```/);
  return match ? match[1].trim() : null;
}

function validateYaml(text: string): { valid: boolean; error?: string } {
  try {
    YAML.parse(text);
    return { valid: true };
  } catch (e) {
    return { valid: false, error: e instanceof Error ? e.message : "Invalid YAML" };
  }
}

// ── Component ──────────────────────────────────────────────────

export default function PlaygroundTab() {
  const [mode, setMode] = useState<Mode>("agent");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [yamlContent, setYamlContent] = useState("");
  const [yamlError, setYamlError] = useState<string | null>(null);
  const [deploying, setDeploying] = useState(false);
  const [deployResult, setDeployResult] = useState<{ ok: boolean; msg: string } | null>(null);
  const [copied, setCopied] = useState(false);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Auto-scroll on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // Update YAML preview when messages change
  useEffect(() => {
    // Find the last assistant message with YAML
    for (let i = messages.length - 1; i >= 0; i--) {
      if (messages[i].role === "assistant") {
        const yaml = extractYaml(messages[i].content);
        if (yaml) {
          setYamlContent(yaml);
          const result = validateYaml(yaml);
          setYamlError(result.valid ? null : result.error ?? "Invalid YAML");
          setDeployResult(null);
          return;
        }
      }
    }
  }, [messages]);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const task = input.trim();
      if (!task || loading) return;

      const contextPrefix =
        mode === "agent"
          ? "I want to create an AGENT config. "
          : "I want to create a SKILL config. ";

      const userMsg: ChatMessage = {
        id: `u-${Date.now()}`,
        role: "user",
        content: task,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, userMsg]);
      setInput("");
      setLoading(true);

      try {
        const fullPrompt =
          messages.length === 0 ? contextPrefix + task : task;
        const response = await executeAgent("playground-assistant", fullPrompt);

        const assistantMsg: ChatMessage = {
          id: `a-${Date.now()}`,
          role: "assistant",
          content: response.output,
          timestamp: Date.now(),
        };
        setMessages((prev) => [...prev, assistantMsg]);
      } catch (err) {
        const errorMsg: ChatMessage = {
          id: `e-${Date.now()}`,
          role: "error",
          content: err instanceof Error ? err.message : "Failed to get response",
          timestamp: Date.now(),
        };
        setMessages((prev) => [...prev, errorMsg]);
      } finally {
        setLoading(false);
        inputRef.current?.focus();
      }
    },
    [input, loading, mode, messages.length]
  );

  const handleDeploy = useCallback(async () => {
    if (!yamlContent || yamlError) return;
    setDeploying(true);
    setDeployResult(null);

    try {
      const parsed = YAML.parse(yamlContent);

      if (mode === "agent") {
        await createAgent(parsed);
        setDeployResult({ ok: true, msg: "Agent deployed successfully" });
      } else {
        // Skill deployment — POST to /api/bundeswehr/skills
        const res = await fetch("/api/bundeswehr/skills", {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(parsed),
        });
        if (!res.ok) {
          const body = await res.text();
          throw new Error(body || `HTTP ${res.status}`);
        }
        setDeployResult({ ok: true, msg: "Skill deployed successfully" });
      }
    } catch (err) {
      setDeployResult({
        ok: false,
        msg: err instanceof Error ? err.message : "Deploy failed",
      });
    } finally {
      setDeploying(false);
    }
  }, [yamlContent, yamlError, mode]);

  const handleCopy = useCallback(() => {
    if (!yamlContent) return;
    navigator.clipboard.writeText(yamlContent).then(() => {
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    });
  }, [yamlContent]);

  const canDeploy = yamlContent.length > 0 && !yamlError && !deploying;

  return (
    <div className="flex gap-4 h-[calc(100vh-200px)] min-h-[400px]">
      {/* Left: Chat pane */}
      <div
        className="w-1/2 flex flex-col rounded-xl overflow-hidden"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: "1px solid rgba(251, 191, 36, 0.1)",
        }}
      >
        {/* Chat header with mode toggle */}
        <div
          className="px-4 py-3 flex items-center justify-between shrink-0"
          style={{ borderBottom: "1px solid rgba(251, 191, 36, 0.08)" }}
        >
          <span
            className="text-[10px] font-bold uppercase tracking-[0.15em]"
            style={{ color: "#fbbf24" }}
          >
            Playground Chat
          </span>

          {/* Mode toggle */}
          <div
            className="flex rounded-lg overflow-hidden"
            style={{ border: "1px solid rgba(251, 191, 36, 0.15)" }}
          >
            <button
              onClick={() => setMode("agent")}
              className="px-3 py-1 text-[10px] font-bold tracking-wider uppercase transition-all"
              style={{
                background: mode === "agent" ? "rgba(251, 191, 36, 0.15)" : "transparent",
                color: mode === "agent" ? "#fbbf24" : "#64748b",
              }}
            >
              Agent
            </button>
            <button
              onClick={() => setMode("skill")}
              className="px-3 py-1 text-[10px] font-bold tracking-wider uppercase transition-all"
              style={{
                background: mode === "skill" ? "rgba(168, 85, 247, 0.15)" : "transparent",
                color: mode === "skill" ? "#a855f7" : "#64748b",
                borderLeft: "1px solid rgba(251, 191, 36, 0.15)",
              }}
            >
              Skill
            </button>
          </div>
        </div>

        {/* Messages */}
        <div className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
          {messages.length === 0 && (
            <div className="flex items-center justify-center h-full">
              <div className="text-center max-w-xs">
                <div
                  className="text-xs font-bold tracking-wider uppercase mb-2"
                  style={{ color: mode === "agent" ? "#fbbf24" : "#a855f7" }}
                >
                  {mode === "agent" ? "Agent Builder" : "Skill Builder"}
                </div>
                <p className="text-slate-500 text-[11px] leading-relaxed font-mono">
                  {mode === "agent"
                    ? "Describe the agent you want to create. The AI will generate a YAML config for you."
                    : "Describe the skill you want to create. The AI will generate a YAML config for you."}
                </p>
              </div>
            </div>
          )}

          {messages.map((msg) => (
            <PlaygroundBubble key={msg.id} message={msg} />
          ))}

          {loading && (
            <div className="flex items-center gap-3 px-3 py-2">
              <Spinner />
              <span className="text-xs text-slate-500 animate-pulse font-mono">
                Generating {mode} config...
              </span>
            </div>
          )}

          <div ref={messagesEndRef} />
        </div>

        {/* Input */}
        <form
          onSubmit={handleSubmit}
          className="px-4 py-3 shrink-0"
          style={{ borderTop: "1px solid rgba(251, 191, 36, 0.08)" }}
        >
          <div
            className="flex items-center gap-2 rounded-lg px-3 py-2"
            style={{
              background: "rgba(251, 191, 36, 0.03)",
              border: "1px solid rgba(251, 191, 36, 0.1)",
            }}
          >
            <input
              ref={inputRef}
              type="text"
              value={input}
              onChange={(e) => setInput(e.target.value)}
              placeholder={
                mode === "agent"
                  ? "Describe the agent you want..."
                  : "Describe the skill you want..."
              }
              disabled={loading}
              className="flex-1 bg-transparent text-xs text-slate-200 placeholder:text-slate-600 outline-none font-mono"
            />
            <button
              type="submit"
              disabled={loading || !input.trim()}
              className="px-3 py-1 rounded-lg text-[10px] font-bold tracking-wider uppercase transition-all disabled:opacity-30"
              style={{
                background: "rgba(251, 191, 36, 0.15)",
                border: "1px solid rgba(251, 191, 36, 0.3)",
                color: "#fbbf24",
              }}
            >
              {loading ? "..." : "Send"}
            </button>
          </div>
        </form>
      </div>

      {/* Right: YAML preview pane */}
      <div
        className="w-1/2 flex flex-col rounded-xl overflow-hidden"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: `1px solid ${yamlError ? "rgba(255, 71, 87, 0.2)" : "rgba(0, 240, 255, 0.1)"}`,
        }}
      >
        {/* Preview header */}
        <div
          className="px-4 py-3 flex items-center justify-between shrink-0"
          style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.08)" }}
        >
          <div className="flex items-center gap-2">
            <span
              className="text-[10px] font-bold uppercase tracking-[0.15em]"
              style={{ color: "#00f0ff" }}
            >
              YAML Preview
            </span>
            {yamlContent && !yamlError && (
              <span
                className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                style={{ background: "rgba(6, 214, 160, 0.1)", color: "#06d6a0" }}
              >
                valid
              </span>
            )}
            {yamlError && (
              <span
                className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                style={{ background: "rgba(255, 71, 87, 0.1)", color: "#ff4757" }}
              >
                invalid
              </span>
            )}
          </div>

          {/* Copy button */}
          <button
            onClick={handleCopy}
            disabled={!yamlContent}
            className="text-[10px] font-mono px-2 py-1 rounded transition-all disabled:opacity-30 hover:opacity-80"
            style={{
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.1)",
              color: copied ? "#06d6a0" : "#00f0ff",
            }}
          >
            {copied ? "Copied!" : "Copy YAML"}
          </button>
        </div>

        {/* YAML content */}
        <div className="flex-1 overflow-y-auto">
          {!yamlContent ? (
            <div className="flex items-center justify-center h-full">
              <div className="text-center">
                <div className="text-slate-600 text-xs font-mono">
                  YAML will appear here as the AI generates config
                </div>
              </div>
            </div>
          ) : (
            <div className="text-sm">
              <SyntaxHighlighter
                language="yaml"
                style={atomOneDark}
                customStyle={{
                  background: "transparent",
                  padding: "1rem",
                  margin: 0,
                  fontSize: "11px",
                  lineHeight: "1.6",
                }}
              >
                {yamlContent}
              </SyntaxHighlighter>
            </div>
          )}

          {/* YAML error */}
          {yamlError && (
            <div
              className="mx-4 mb-4 px-3 py-2 rounded-lg text-[11px] font-mono"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
                color: "#ff4757",
              }}
            >
              Parse error: {yamlError}
            </div>
          )}
        </div>

        {/* Deploy bar */}
        <div
          className="px-4 py-3 flex items-center justify-between shrink-0"
          style={{ borderTop: "1px solid rgba(0, 240, 255, 0.08)" }}
        >
          {/* Deploy result */}
          {deployResult && (
            <span
              className="text-[10px] font-mono"
              style={{ color: deployResult.ok ? "#06d6a0" : "#ff4757" }}
            >
              {deployResult.msg}
            </span>
          )}
          {!deployResult && <span />}

          <button
            onClick={handleDeploy}
            disabled={!canDeploy}
            className="px-4 py-1.5 rounded-lg text-[10px] font-bold tracking-wider uppercase transition-all disabled:opacity-30 hover:opacity-90"
            style={{
              background:
                mode === "agent"
                  ? "rgba(251, 191, 36, 0.15)"
                  : "rgba(168, 85, 247, 0.15)",
              border: `1px solid ${mode === "agent" ? "rgba(251, 191, 36, 0.3)" : "rgba(168, 85, 247, 0.3)"}`,
              color: mode === "agent" ? "#fbbf24" : "#a855f7",
            }}
          >
            {deploying
              ? "Deploying..."
              : `Deploy ${mode === "agent" ? "Agent" : "Skill"}`}
          </button>
        </div>
      </div>
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────

function PlaygroundBubble({ message }: { message: ChatMessage }) {
  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-[85%] rounded-xl px-3 py-2"
          style={{
            background: "rgba(251, 191, 36, 0.08)",
            border: "1px solid rgba(251, 191, 36, 0.15)",
          }}
        >
          <p className="text-[11px] text-slate-200 font-mono whitespace-pre-wrap">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  if (message.role === "error") {
    return (
      <div className="flex justify-start">
        <div
          className="max-w-[85%] rounded-xl px-3 py-2"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.2)",
          }}
        >
          <span
            className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
            style={{ background: "rgba(255, 71, 87, 0.15)", color: "#ff4757" }}
          >
            Error
          </span>
          <p className="text-[11px] text-red-300/80 font-mono whitespace-pre-wrap mt-1">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  // Assistant
  // Strip YAML blocks for chat display (YAML shown in preview pane)
  const displayContent = message.content
    .replace(/```ya?ml\n[\s\S]*?\n```/g, "[YAML config shown in preview panel →]")
    .trim();

  return (
    <div className="flex justify-start">
      <div
        className="max-w-[90%] rounded-xl px-3 py-2"
        style={{
          background: "rgba(0, 240, 255, 0.04)",
          border: "1px solid rgba(0, 240, 255, 0.1)",
        }}
      >
        <p className="text-[11px] text-slate-300 font-mono whitespace-pre-wrap leading-relaxed">
          {displayContent}
        </p>
      </div>
    </div>
  );
}

function Spinner() {
  return (
    <div
      className="w-3 h-3 rounded-full animate-spin"
      style={{
        border: "2px solid rgba(251, 191, 36, 0.1)",
        borderTopColor: "#fbbf24",
      }}
    />
  );
}
