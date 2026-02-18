"use client";

import { useState, useCallback, useRef } from "react";

// ── Types ──────────────────────────────────────────────────────────

export type StreamEventType =
  | "agent_start"
  | "agent_token"
  | "agent_step"
  | "agent_complete"
  | "agent_error";

export interface StreamMessage {
  type: StreamEventType;
  data: Record<string, unknown>;
  timestamp: string;
}

export interface AgentStreamState {
  /** Trigger an agent execution with SSE streaming. */
  execute: (agentId: string, input: string, pipelineId?: string) => void;
  /** All received SSE events in order. */
  messages: StreamMessage[];
  /** Accumulated text output from agent_token events. */
  output: string;
  /** Whether an execution is currently in progress. */
  isRunning: boolean;
  /** Last error message, if any. */
  error: string | null;
  /** Reset state for a new execution. */
  reset: () => void;
}

// ── Hook ───────────────────────────────────────────────────────────

export function useAgentStream(): AgentStreamState {
  const [messages, setMessages] = useState<StreamMessage[]>([]);
  const [output, setOutput] = useState("");
  const [isRunning, setIsRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const abortRef = useRef<AbortController | null>(null);

  const reset = useCallback(() => {
    abortRef.current?.abort();
    setMessages([]);
    setOutput("");
    setIsRunning(false);
    setError(null);
  }, []);

  const execute = useCallback(
    (agentId: string, input: string, pipelineId?: string) => {
      // Abort any previous execution
      abortRef.current?.abort();
      const controller = new AbortController();
      abortRef.current = controller;

      // Reset state
      setMessages([]);
      setOutput("");
      setError(null);
      setIsRunning(true);

      // Start streaming
      (async () => {
        try {
          const res = await fetch("/api/stille-post/execute", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({
              agent_id: agentId,
              input,
              pipeline_id: pipelineId,
            }),
            signal: controller.signal,
          });

          if (!res.ok) {
            const errBody = await res.json().catch(() => ({ error: res.statusText }));
            setError(errBody.error || `HTTP ${res.status}`);
            setIsRunning(false);
            return;
          }

          const reader = res.body?.getReader();
          if (!reader) {
            setError("No response body");
            setIsRunning(false);
            return;
          }

          const decoder = new TextDecoder();
          let buffer = "";

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
                  const msg: StreamMessage = {
                    type: currentEvent as StreamEventType,
                    data,
                    timestamp: data.timestamp || new Date().toISOString(),
                  };

                  setMessages((prev) => [...prev, msg]);

                  if (currentEvent === "agent_token" && data.token) {
                    setOutput((prev) => prev + data.token);
                  }

                  if (currentEvent === "agent_error") {
                    setError(data.error || "Unknown agent error");
                  }
                } catch {
                  // Skip malformed JSON
                }
                currentEvent = "";
              }
            }
          }
        } catch (err) {
          if ((err as Error).name !== "AbortError") {
            setError(
              err instanceof Error ? err.message : "Stream connection failed",
            );
          }
        } finally {
          setIsRunning(false);
        }
      })();
    },
    [],
  );

  return { execute, messages, output, isRunning, error, reset };
}
