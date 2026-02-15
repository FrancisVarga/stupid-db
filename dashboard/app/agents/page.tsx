"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import Link from "next/link";
import {
  fetchAgents,
  fetchStrategies,
  executeAgent,
  executeTeam,
  type AgentInfo,
  type AgentResponse,
  type TeamResponse,
  type StrategyInfo,
} from "@/lib/api";

// ── Types ──────────────────────────────────────────────────────────────

type MessageRole = "user" | "agent" | "team" | "error";

interface ChatMessage {
  id: string;
  role: MessageRole;
  content: string;
  timestamp: number;
  agentName?: string;
  status?: string;
  executionTimeMs?: number;
  teamOutputs?: Record<string, string>;
  agentsUsed?: string[];
  strategy?: string;
}

type ExecutionMode = "agent" | "team";

// ── Constants ──────────────────────────────────────────────────────────

const TIER_COLORS: Record<string, string> = {
  T1: "#00f0ff",
  T2: "#a855f7",
  T3: "#f472b6",
};

const STATUS_COLORS: Record<string, string> = {
  success: "#06d6a0",
  completed: "#06d6a0",
  error: "#ff4757",
  failed: "#ff4757",
  running: "#ffe600",
  pending: "#64748b",
};

function statusColor(status: string): string {
  const lower = status.toLowerCase();
  return STATUS_COLORS[lower] || "#64748b";
}

function tierColor(tier: string): string {
  return TIER_COLORS[tier] || "#64748b";
}

function uniqueId(): string {
  return `${Date.now()}-${Math.random().toString(36).slice(2, 8)}`;
}

// ── Page Component ─────────────────────────────────────────────────────

export default function AgentsPage() {
  const [agents, setAgents] = useState<AgentInfo[]>([]);
  const [strategies, setStrategies] = useState<StrategyInfo[]>([]);
  const [selectedAgent, setSelectedAgent] = useState<string>("");
  const [selectedStrategy, setSelectedStrategy] = useState<string>("architect_only");
  const [mode, setMode] = useState<ExecutionMode>("agent");
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [input, setInput] = useState("");
  const [loading, setLoading] = useState(false);
  const [initError, setInitError] = useState<string | null>(null);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load agents and strategies on mount
  useEffect(() => {
    async function init() {
      try {
        const [agentList, strategyList] = await Promise.allSettled([
          fetchAgents(),
          fetchStrategies(),
        ]);
        if (agentList.status === "fulfilled") {
          setAgents(agentList.value);
          if (agentList.value.length > 0) {
            setSelectedAgent(agentList.value[0].name);
          }
        }
        if (strategyList.status === "fulfilled") {
          setStrategies(strategyList.value);
          if (strategyList.value.length > 0) {
            setSelectedStrategy(strategyList.value[0].name);
          }
        }
      } catch (e) {
        setInitError(`Failed to load agents: ${(e as Error).message}`);
      }
    }
    init();
  }, []);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const task = input.trim();
      if (!task || loading) return;

      // Add user message
      const userMsg: ChatMessage = {
        id: uniqueId(),
        role: "user",
        content: task,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, userMsg]);
      setInput("");
      setLoading(true);

      try {
        if (mode === "agent") {
          const res: AgentResponse = await executeAgent(selectedAgent, task);
          const agentMsg: ChatMessage = {
            id: uniqueId(),
            role: "agent",
            content: res.output,
            timestamp: Date.now(),
            agentName: res.agent_name,
            status: res.status,
            executionTimeMs: res.execution_time_ms,
          };
          setMessages((prev) => [...prev, agentMsg]);
        } else {
          const res: TeamResponse = await executeTeam(task, selectedStrategy);
          const teamMsg: ChatMessage = {
            id: uniqueId(),
            role: "team",
            content: `Team completed task using ${res.agents_used.length} agent(s).`,
            timestamp: Date.now(),
            status: res.status,
            executionTimeMs: res.execution_time_ms,
            teamOutputs: res.outputs,
            agentsUsed: res.agents_used,
            strategy: res.strategy,
          };
          setMessages((prev) => [...prev, teamMsg]);
        }
      } catch (err) {
        const errorMsg: ChatMessage = {
          id: uniqueId(),
          role: "error",
          content: (err as Error).message || "Unknown error occurred",
          timestamp: Date.now(),
        };
        setMessages((prev) => [...prev, errorMsg]);
      } finally {
        setLoading(false);
        inputRef.current?.focus();
      }
    },
    [input, loading, mode, selectedAgent, selectedStrategy]
  );

  // ── Init error state ────────────────────────────────────────────────

  if (initError) {
    return (
      <div className="min-h-screen flex items-center justify-center">
        <div
          className="rounded-xl p-8 max-w-md"
          style={{
            background: "linear-gradient(135deg, #1a0a0a 0%, #0d0606 100%)",
            border: "1px solid rgba(255, 71, 87, 0.2)",
            boxShadow: "0 0 40px rgba(255, 71, 87, 0.05)",
          }}
        >
          <h2 className="text-red-400 font-bold text-lg tracking-wide">
            CONNECTION FAILED
          </h2>
          <p className="text-red-300/70 mt-2 text-sm">{initError}</p>
          <p className="text-slate-500 text-xs mt-4">
            Start the server:{" "}
            <code className="text-slate-300 bg-slate-800/50 px-1.5 py-0.5 rounded">
              stupid-server serve
            </code>
          </p>
        </div>
      </div>
    );
  }

  // ── Render ──────────────────────────────────────────────────────────

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link
            href="/"
            className="text-lg font-bold tracking-wider hover:opacity-80 transition-opacity"
            style={{ color: "#00f0ff" }}
          >
            stupid-db
          </Link>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            agents
          </span>
        </div>
        <Link
          href="/"
          className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full text-xs font-medium tracking-wide hover:opacity-80 transition-opacity"
          style={{
            background: "rgba(0, 240, 255, 0.08)",
            border: "1px solid rgba(0, 240, 255, 0.2)",
            color: "#00f0ff",
          }}
        >
          Dashboard
        </Link>
      </header>

      {/* Controls bar */}
      <div
        className="px-6 py-3 flex items-center gap-4 shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.05)",
          background: "rgba(0, 240, 255, 0.01)",
        }}
      >
        {/* Mode toggle */}
        <div className="flex rounded-lg overflow-hidden" style={{ border: "1px solid rgba(0, 240, 255, 0.15)" }}>
          <button
            onClick={() => setMode("agent")}
            className="px-3 py-1.5 text-xs font-bold tracking-wider uppercase transition-all"
            style={{
              background: mode === "agent" ? "rgba(0, 240, 255, 0.15)" : "transparent",
              color: mode === "agent" ? "#00f0ff" : "#64748b",
            }}
          >
            Single Agent
          </button>
          <button
            onClick={() => setMode("team")}
            className="px-3 py-1.5 text-xs font-bold tracking-wider uppercase transition-all"
            style={{
              background: mode === "team" ? "rgba(168, 85, 247, 0.15)" : "transparent",
              color: mode === "team" ? "#a855f7" : "#64748b",
              borderLeft: "1px solid rgba(0, 240, 255, 0.15)",
            }}
          >
            Team
          </button>
        </div>

        {/* Agent selector (single mode) */}
        {mode === "agent" && (
          <div className="flex items-center gap-2">
            <label className="text-[10px] text-slate-500 tracking-widest uppercase">
              Agent
            </label>
            <select
              value={selectedAgent}
              onChange={(e) => setSelectedAgent(e.target.value)}
              className="rounded-lg px-3 py-1.5 text-xs font-mono appearance-none cursor-pointer outline-none"
              style={{
                background: "rgba(0, 240, 255, 0.06)",
                border: "1px solid rgba(0, 240, 255, 0.15)",
                color: "#e2e8f0",
              }}
            >
              {agents.map((a) => (
                <option key={a.name} value={a.name}>
                  [{a.tier}] {a.name}
                </option>
              ))}
            </select>
            {agents.find((a) => a.name === selectedAgent) && (
              <AgentBadge agent={agents.find((a) => a.name === selectedAgent)!} />
            )}
          </div>
        )}

        {/* Strategy selector (team mode) */}
        {mode === "team" && (
          <div className="flex items-center gap-2">
            <label className="text-[10px] text-slate-500 tracking-widest uppercase">
              Strategy
            </label>
            <select
              value={selectedStrategy}
              onChange={(e) => setSelectedStrategy(e.target.value)}
              className="rounded-lg px-3 py-1.5 text-xs font-mono appearance-none cursor-pointer outline-none"
              style={{
                background: "rgba(168, 85, 247, 0.06)",
                border: "1px solid rgba(168, 85, 247, 0.15)",
                color: "#e2e8f0",
              }}
            >
              {strategies.length > 0 ? (
                strategies.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name}
                  </option>
                ))
              ) : (
                <>
                  <option value="architect_only">architect_only</option>
                  <option value="leads_only">leads_only</option>
                  <option value="full_hierarchy">full_hierarchy</option>
                </>
              )}
            </select>
          </div>
        )}
      </div>

      {/* Messages area */}
      <div className="flex-1 overflow-y-auto px-6 py-4 space-y-4">
        {messages.length === 0 && (
          <div className="flex items-center justify-center h-full">
            <div className="text-center max-w-md">
              <div
                className="text-sm font-bold tracking-wider uppercase mb-2"
                style={{ color: "#00f0ff" }}
              >
                {mode === "agent" ? "Agent Chat" : "Team Chat"}
              </div>
              <p className="text-slate-500 text-xs leading-relaxed">
                {mode === "agent"
                  ? "Send a task to the selected agent. The agent will process your request and return a response."
                  : "Send a task to the team. Multiple agents will collaborate using the selected strategy."}
              </p>
              {agents.length > 0 && (
                <div className="mt-4 flex flex-wrap justify-center gap-1.5">
                  {agents.map((a) => (
                    <span
                      key={a.name}
                      className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-[10px] font-mono"
                      style={{
                        background: `${tierColor(a.tier)}10`,
                        border: `1px solid ${tierColor(a.tier)}25`,
                        color: tierColor(a.tier),
                      }}
                    >
                      <span className="font-bold">{a.tier}</span>
                      {a.name}
                    </span>
                  ))}
                </div>
              )}
            </div>
          </div>
        )}

        {messages.map((msg) => (
          <MessageBubble key={msg.id} message={msg} />
        ))}

        {loading && (
          <div className="flex items-center gap-3 px-4 py-3">
            <Spinner />
            <span className="text-xs text-slate-500 animate-pulse">
              {mode === "agent"
                ? `${selectedAgent} is processing...`
                : `Team executing with ${selectedStrategy} strategy...`}
            </span>
          </div>
        )}

        <div ref={messagesEndRef} />
      </div>

      {/* Input bar */}
      <form
        onSubmit={handleSubmit}
        className="px-6 py-4 shrink-0"
        style={{
          borderTop: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(0deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div
          className="flex items-center gap-3 rounded-xl px-4 py-3"
          style={{
            background: "rgba(0, 240, 255, 0.03)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
          }}
        >
          <input
            ref={inputRef}
            type="text"
            value={input}
            onChange={(e) => setInput(e.target.value)}
            placeholder={
              mode === "agent"
                ? `Ask ${selectedAgent || "an agent"}...`
                : `Describe a task for the team...`
            }
            disabled={loading}
            className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-600 outline-none font-mono"
          />
          <button
            type="submit"
            disabled={loading || !input.trim()}
            className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-all disabled:opacity-30"
            style={{
              background:
                mode === "agent"
                  ? "rgba(0, 240, 255, 0.15)"
                  : "rgba(168, 85, 247, 0.15)",
              border: `1px solid ${mode === "agent" ? "rgba(0, 240, 255, 0.3)" : "rgba(168, 85, 247, 0.3)"}`,
              color: mode === "agent" ? "#00f0ff" : "#a855f7",
            }}
          >
            {loading ? "Running..." : "Send"}
          </button>
        </div>
      </form>
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────

function AgentBadge({ agent }: { agent: AgentInfo }) {
  const color = tierColor(agent.tier);
  return (
    <span
      className="inline-flex items-center gap-1 px-2 py-0.5 rounded text-[10px] font-mono"
      style={{
        background: `${color}10`,
        border: `1px solid ${color}20`,
        color,
      }}
      title={agent.description}
    >
      <span className="font-bold">{agent.tier}</span>
      <span className="text-slate-500 hidden sm:inline">{agent.description}</span>
    </span>
  );
}

function MessageBubble({ message }: { message: ChatMessage }) {
  const [expandedAgents, setExpandedAgents] = useState<Set<string>>(new Set());

  const toggleAgent = (name: string) => {
    setExpandedAgents((prev) => {
      const next = new Set(prev);
      if (next.has(name)) next.delete(name);
      else next.add(name);
      return next;
    });
  };

  if (message.role === "user") {
    return (
      <div className="flex justify-end">
        <div
          className="max-w-[70%] rounded-xl px-4 py-3"
          style={{
            background: "rgba(0, 240, 255, 0.08)",
            border: "1px solid rgba(0, 240, 255, 0.15)",
          }}
        >
          <p className="text-sm text-slate-200 font-mono whitespace-pre-wrap">
            {message.content}
          </p>
          <div className="text-[9px] text-slate-600 mt-1 text-right font-mono">
            {new Date(message.timestamp).toLocaleTimeString()}
          </div>
        </div>
      </div>
    );
  }

  if (message.role === "error") {
    return (
      <div className="flex justify-start">
        <div
          className="max-w-[70%] rounded-xl px-4 py-3"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.2)",
          }}
        >
          <div className="flex items-center gap-2 mb-1">
            <span
              className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
              style={{ background: "rgba(255, 71, 87, 0.15)", color: "#ff4757" }}
            >
              Error
            </span>
          </div>
          <p className="text-sm text-red-300/80 font-mono whitespace-pre-wrap">
            {message.content}
          </p>
        </div>
      </div>
    );
  }

  if (message.role === "agent") {
    const sColor = statusColor(message.status || "");
    return (
      <div className="flex justify-start">
        <div
          className="max-w-[80%] rounded-xl px-4 py-3"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
          }}
        >
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs font-bold font-mono" style={{ color: "#00f0ff" }}>
              {message.agentName}
            </span>
            {message.status && (
              <span
                className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
                style={{ background: `${sColor}15`, color: sColor }}
              >
                {message.status}
              </span>
            )}
            {message.executionTimeMs !== undefined && (
              <span className="text-[9px] font-mono text-slate-600">
                {message.executionTimeMs}ms
              </span>
            )}
          </div>
          <p className="text-sm text-slate-300 font-mono whitespace-pre-wrap leading-relaxed">
            {message.content}
          </p>
          <div className="text-[9px] text-slate-600 mt-2 font-mono">
            {new Date(message.timestamp).toLocaleTimeString()}
          </div>
        </div>
      </div>
    );
  }

  // Team response
  if (message.role === "team") {
    const sColor = statusColor(message.status || "");
    return (
      <div className="flex justify-start">
        <div
          className="max-w-[85%] rounded-xl px-4 py-3"
          style={{
            background: "linear-gradient(135deg, #0c0818 0%, #110c27 100%)",
            border: "1px solid rgba(168, 85, 247, 0.15)",
          }}
        >
          {/* Team header */}
          <div className="flex items-center gap-2 mb-2">
            <span className="text-xs font-bold font-mono" style={{ color: "#a855f7" }}>
              Team
            </span>
            {message.strategy && (
              <span
                className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                style={{
                  background: "rgba(168, 85, 247, 0.1)",
                  color: "#a855f7",
                  border: "1px solid rgba(168, 85, 247, 0.2)",
                }}
              >
                {message.strategy}
              </span>
            )}
            {message.status && (
              <span
                className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
                style={{ background: `${sColor}15`, color: sColor }}
              >
                {message.status}
              </span>
            )}
            {message.executionTimeMs !== undefined && (
              <span className="text-[9px] font-mono text-slate-600">
                {message.executionTimeMs}ms
              </span>
            )}
          </div>

          <p className="text-sm text-slate-300 font-mono mb-3">
            {message.content}
          </p>

          {/* Agents used */}
          {message.agentsUsed && message.agentsUsed.length > 0 && (
            <div className="flex flex-wrap gap-1 mb-3">
              {message.agentsUsed.map((name) => (
                <span
                  key={name}
                  className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                  style={{
                    background: "rgba(0, 240, 255, 0.08)",
                    border: "1px solid rgba(0, 240, 255, 0.15)",
                    color: "#00f0ff",
                  }}
                >
                  {name}
                </span>
              ))}
            </div>
          )}

          {/* Collapsible agent outputs */}
          {message.teamOutputs &&
            Object.keys(message.teamOutputs).length > 0 && (
              <div className="space-y-1.5">
                {Object.entries(message.teamOutputs).map(([agentName, output]) => {
                  const isExpanded = expandedAgents.has(agentName);
                  return (
                    <div
                      key={agentName}
                      className="rounded-lg overflow-hidden"
                      style={{
                        border: "1px solid rgba(0, 240, 255, 0.08)",
                        background: "rgba(0, 0, 0, 0.2)",
                      }}
                    >
                      <button
                        onClick={() => toggleAgent(agentName)}
                        className="w-full flex items-center justify-between px-3 py-2 text-left hover:opacity-80 transition-opacity"
                      >
                        <span className="text-[10px] font-bold font-mono" style={{ color: "#00f0ff" }}>
                          {agentName}
                        </span>
                        <span className="text-[10px] text-slate-600">
                          {isExpanded ? "\u25B2" : "\u25BC"}
                        </span>
                      </button>
                      {isExpanded && (
                        <div
                          className="px-3 pb-3"
                          style={{ borderTop: "1px solid rgba(0, 240, 255, 0.05)" }}
                        >
                          <p className="text-xs text-slate-400 font-mono whitespace-pre-wrap leading-relaxed pt-2">
                            {output}
                          </p>
                        </div>
                      )}
                    </div>
                  );
                })}
              </div>
            )}

          <div className="text-[9px] text-slate-600 mt-2 font-mono">
            {new Date(message.timestamp).toLocaleTimeString()}
          </div>
        </div>
      </div>
    );
  }

  return null;
}

function Spinner() {
  return (
    <div
      className="w-4 h-4 rounded-full animate-spin"
      style={{
        border: "2px solid rgba(0, 240, 255, 0.1)",
        borderTopColor: "#00f0ff",
      }}
    />
  );
}
