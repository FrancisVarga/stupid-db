"use client";

import { useEffect, useRef, useState } from "react";
import { useVillaStore } from "@/lib/villa/store";
import type { ChatMessage, LayoutAction, VillaSuggestRequest, VillaSuggestResponse } from "@/lib/villa/types";
import { validateLayoutActions } from "@/lib/villa/error-handling";
import { useReducedMotion } from "@/lib/villa/useReducedMotion";

// Injected as a module-level <style> to avoid re-renders
const chatSlideStyles = `
@keyframes villa-chat-slide-in {
  from { transform: translateX(100%); }
  to { transform: translateX(0); }
}
.villa-chat-slide-in {
  animation: villa-chat-slide-in 300ms ease-out both;
}
@media (prefers-reduced-motion: reduce) {
  .villa-chat-slide-in { animation: none; }
}
`;
if (typeof document !== "undefined") {
  const id = "villa-chat-slide-css";
  if (!document.getElementById(id)) {
    const s = document.createElement("style");
    s.id = id;
    s.textContent = chatSlideStyles;
    document.head.appendChild(s);
  }
}

let msgSeq = 0;
function makeId(): string { return `msg-${Date.now()}-${++msgSeq}`; }

const SUGGEST_TIMEOUT_MS = 10_000;

function actionLabel(a: LayoutAction): string {
  switch (a.action) {
    case "add":
      return `Add ${a.widget?.type ?? "widget"} — ${a.widget?.title ?? "Untitled"}`;
    case "remove":
      return `Remove ${a.widgetId ?? "widget"}`;
    case "resize":
      return `Resize ${a.widgetId ?? "widget"} to ${a.dimensions?.w ?? "?"}×${a.dimensions?.h ?? "?"}`;
    case "move":
      return `Move ${a.widgetId ?? "widget"}`;
  }
}

function ActionCard({
  actions,
  onApply,
}: {
  actions: LayoutAction[];
  onApply: (actions: LayoutAction[]) => void;
}) {
  const [applied, setApplied] = useState(false);

  return (
    <div
      className="mt-2 rounded-lg p-2.5 space-y-1.5"
      style={{
        background: applied ? "rgba(100, 116, 139, 0.08)" : "rgba(0, 212, 255, 0.06)",
        border: `1px solid ${applied ? "rgba(100, 116, 139, 0.15)" : "rgba(0, 212, 255, 0.15)"}`,
      }}
    >
      {actions.map((a, i) => (
        <div key={i} className="text-[10px] font-mono text-slate-400">
          {actionLabel(a)}
        </div>
      ))}
      <button
        disabled={applied}
        onClick={() => {
          onApply(actions);
          setApplied(true);
        }}
        className="mt-1 px-3 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-opacity"
        style={{
          background: applied ? "rgba(100, 116, 139, 0.1)" : "rgba(0, 212, 255, 0.12)",
          border: `1px solid ${applied ? "rgba(100, 116, 139, 0.2)" : "rgba(0, 212, 255, 0.25)"}`,
          color: applied ? "#64748b" : "#00d4ff",
          cursor: applied ? "default" : "pointer",
          opacity: applied ? 0.5 : 1,
        }}
      >
        {applied ? "Applied" : "Apply"}
      </button>
    </div>
  );
}

function MessageBubble({
  message,
  onApply,
}: {
  message: ChatMessage;
  onApply: (actions: LayoutAction[]) => void;
}) {
  const isUser = message.role === "user";

  return (
    <div className={`flex ${isUser ? "justify-end" : "justify-start"}`}>
      <div
        className="max-w-[85%] rounded-xl px-3 py-2"
        style={{
          background: isUser ? "rgba(0, 212, 255, 0.1)" : "rgba(30, 41, 59, 0.6)",
          border: `1px solid ${isUser ? "rgba(0, 212, 255, 0.2)" : "rgba(100, 116, 139, 0.15)"}`,
        }}
      >
        <p className="text-xs text-slate-300 leading-relaxed whitespace-pre-wrap">
          {message.content}
        </p>
        {message.actions && message.actions.length > 0 && (
          <ActionCard actions={message.actions} onApply={onApply} />
        )}
        <div className="text-[9px] text-slate-600 mt-1 font-mono">
          {new Date(message.timestamp).toLocaleTimeString([], {
            hour: "2-digit",
            minute: "2-digit",
          })}
        </div>
      </div>
    </div>
  );
}

export default function ChatPanel() {
  const { chatMessages, isChatOpen, widgets, addChatMessage, toggleChat, applyActions } =
    useVillaStore();
  const [input, setInput] = useState("");
  const [sending, setSending] = useState(false);
  const reducedMotion = useReducedMotion();
  const bottomRef = useRef<HTMLDivElement>(null);
  const abortRef = useRef<AbortController | null>(null);

  // Auto-scroll to bottom on new message
  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: reducedMotion ? "instant" : "smooth" });
  }, [chatMessages.length]);

  // Cleanup abort controller on unmount
  useEffect(() => () => abortRef.current?.abort(), []);

  const handleSend = async () => {
    const text = input.trim();
    if (!text || sending) return;

    addChatMessage({ id: makeId(), role: "user", content: text, timestamp: Date.now() });
    setInput("");
    setSending(true);

    // Abort any in-flight request
    abortRef.current?.abort();
    const controller = new AbortController();
    abortRef.current = controller;

    // Timeout after 10 seconds
    const timeoutId = setTimeout(() => controller.abort(), SUGGEST_TIMEOUT_MS);

    try {
      const body: VillaSuggestRequest = {
        message: text,
        currentLayout: widgets,
      };

      const res = await fetch("/api/villa/suggest", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!res.ok) {
        throw new Error(`HTTP ${res.status}`);
      }

      const json = await res.json() as Partial<VillaSuggestResponse>;

      if (typeof json.explanation !== "string") {
        console.error("[villa] invalid suggest response: missing explanation", json);
        addChatMessage({
          id: makeId(), role: "assistant",
          content: "Got an unexpected response. Try again.",
          timestamp: Date.now(),
        });
        return;
      }

      const actions = validateLayoutActions(json.actions);

      addChatMessage({
        id: makeId(), role: "assistant",
        content: json.explanation,
        actions: actions.length > 0 ? actions : undefined,
        timestamp: Date.now(),
      });
    } catch (err) {
      clearTimeout(timeoutId);
      let userMessage: string;

      if (err instanceof DOMException && err.name === "AbortError") {
        userMessage = "The AI is taking too long. Try a simpler request.";
      } else if (err instanceof TypeError) {
        // fetch throws TypeError on network failure
        userMessage = "Could not reach the server. Check your connection.";
      } else {
        userMessage = "Got an unexpected response. Try again.";
      }

      console.error("[villa] suggest error:", err);
      addChatMessage({
        id: makeId(), role: "assistant",
        content: userMessage,
        timestamp: Date.now(),
      });
    } finally {
      setSending(false);
    }
  };

  if (!isChatOpen) {
    return <ChatToggleButton onClick={toggleChat} />;
  }

  return (
    <div
      className="fixed top-0 right-0 h-full flex flex-col z-50 villa-chat-slide-in"
      style={{
        width: 400,
        background: "rgba(6, 8, 13, 0.95)",
        borderLeft: "1px solid rgba(0, 212, 255, 0.1)",
        backdropFilter: "blur(12px)",
      }}
    >
      {/* Header */}
      <div
        className="flex items-center justify-between px-4 py-3 shrink-0"
        style={{ borderBottom: "1px solid rgba(0, 212, 255, 0.08)" }}
      >
        <div className="flex items-center gap-2">
          <span
            className="w-2 h-2 rounded-full"
            style={{ background: "#00d4ff", boxShadow: "0 0 8px #00d4ff40" }}
          />
          <span className="text-xs font-bold tracking-wider uppercase text-slate-300">
            Villa Chat
          </span>
        </div>
        <button
          onClick={toggleChat}
          className="text-slate-500 hover:text-slate-300 transition-colors text-sm px-1"
          aria-label="Close chat panel"
          title="Close chat"
        >
          ✕
        </button>
      </div>

      {/* Messages */}
      <div className="flex-1 overflow-y-auto px-3 py-3 space-y-3 min-h-0">
        {chatMessages.length === 0 && (
          <div className="flex items-center justify-center h-full">
            <p className="text-[11px] text-slate-600 font-mono text-center leading-relaxed">
              Ask about your data to get<br />widget suggestions for your layout.
            </p>
          </div>
        )}
        {chatMessages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} onApply={applyActions} />
        ))}
        {sending && (
          <div className="flex justify-start">
            <div
              className="rounded-xl px-3 py-2 flex items-center gap-2"
              style={{
                background: "rgba(30, 41, 59, 0.6)",
                border: "1px solid rgba(100, 116, 139, 0.15)",
              }}
            >
              <span className="flex gap-1">
                {reducedMotion ? (
                  <span className="w-1.5 h-1.5 rounded-full bg-cyan-400 opacity-60" />
                ) : (
                  [0, 1, 2].map((i) => (
                    <span
                      key={i}
                      className="w-1.5 h-1.5 rounded-full bg-cyan-400 animate-bounce"
                      style={{ animationDelay: `${i * 150}ms` }}
                    />
                  ))
                )}
              </span>
              <span className="text-[10px] text-slate-500 font-mono">Thinking...</span>
            </div>
          </div>
        )}
        <div ref={bottomRef} />
      </div>

      {/* Input area */}
      <div
        className="shrink-0 px-3 py-3"
        style={{ borderTop: "1px solid rgba(0, 212, 255, 0.08)" }}
      >
        <div className="flex gap-2">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter" && !e.shiftKey) {
                e.preventDefault();
                handleSend();
              }
            }}
            placeholder="Ask about your data..."
            className="flex-1 text-xs text-slate-300 placeholder-slate-600 rounded-lg px-3 py-2 outline-none transition-colors"
            style={{
              background: "rgba(15, 23, 42, 0.8)",
              border: "1px solid rgba(0, 212, 255, 0.1)",
            }}
            onFocus={(e) => { e.currentTarget.style.borderColor = "rgba(0, 212, 255, 0.3)"; }}
            onBlur={(e) => { e.currentTarget.style.borderColor = "rgba(0, 212, 255, 0.1)"; }}
          />
          <button
            onClick={handleSend}
            disabled={!input.trim() || sending}
            className="px-3 py-2 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-opacity"
            style={{
              background: input.trim() && !sending
                ? "rgba(0, 212, 255, 0.15)"
                : "rgba(100, 116, 139, 0.1)",
              border: `1px solid ${input.trim() && !sending ? "rgba(0, 212, 255, 0.3)" : "rgba(100, 116, 139, 0.15)"}`,
              color: input.trim() && !sending ? "#00d4ff" : "#475569",
              cursor: input.trim() && !sending ? "pointer" : "default",
            }}
            aria-label="Send message"
          >
            {sending ? "..." : "Send"}
          </button>
        </div>
      </div>
    </div>
  );
}

export function ChatToggleButton({ onClick }: { onClick?: () => void }) {
  const toggleChat = useVillaStore((s) => s.toggleChat);

  return (
    <button
      onClick={onClick ?? toggleChat}
      className="fixed bottom-6 right-6 z-50 w-12 h-12 rounded-full flex items-center justify-center transition-transform hover:scale-105"
      style={{
        background: "rgba(0, 212, 255, 0.15)",
        border: "1px solid rgba(0, 212, 255, 0.3)",
        boxShadow: "0 0 20px rgba(0, 212, 255, 0.1)",
      }}
      aria-label="Open chat panel"
    >
      <svg width="20" height="20" viewBox="0 0 24 24" fill="none" stroke="#00d4ff" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
      </svg>
    </button>
  );
}
