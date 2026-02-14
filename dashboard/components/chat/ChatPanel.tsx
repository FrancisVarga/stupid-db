"use client";

import { useState, useRef, useEffect } from "react";

export interface ChatMessage {
  id: string;
  role: "user" | "system";
  content: string;
  timestamp: Date;
}

interface Props {
  messages: ChatMessage[];
  onSend: (message: string) => void;
}

export default function ChatPanel({ messages, onSend }: Props) {
  const [input, setInput] = useState("");
  const bottomRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    bottomRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    const trimmed = input.trim();
    if (!trimmed) return;
    onSend(trimmed);
    setInput("");
  };

  return (
    <div className="flex flex-col h-full">
      {/* Message list */}
      <div className="flex-1 overflow-y-auto px-4 py-3 space-y-3">
        {messages.map((msg) => (
          <div
            key={msg.id}
            className={`flex ${msg.role === "user" ? "justify-end" : "justify-start"}`}
          >
            <div
              className={`max-w-[85%] rounded-xl px-4 py-2.5 text-sm leading-relaxed ${
                msg.role === "user" ? "chat-bubble-user" : "chat-bubble-system"
              }`}
            >
              <div className="whitespace-pre-wrap">{msg.content}</div>
              <div className="text-[10px] mt-1.5 opacity-40 font-mono">
                {msg.timestamp.toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </div>
            </div>
          </div>
        ))}
        <div ref={bottomRef} />
      </div>

      {/* Input */}
      <form
        onSubmit={handleSubmit}
        className="p-3"
        style={{ borderTop: "1px solid rgba(0, 240, 255, 0.08)" }}
      >
        <div className="chat-input-container flex items-center gap-2 rounded-xl px-4 py-2.5">
          <input
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder="Ask about your data..."
            className="flex-1 bg-transparent text-sm text-slate-200 placeholder-slate-600 outline-none"
          />
          <button
            type="submit"
            disabled={!input.trim()}
            className="text-xs font-bold tracking-wider uppercase px-3 py-1.5 rounded-lg transition-all disabled:opacity-20"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
            }}
          >
            Send
          </button>
        </div>
      </form>
    </div>
  );
}
