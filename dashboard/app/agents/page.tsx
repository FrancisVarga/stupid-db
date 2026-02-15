"use client";

import { useEffect, useState, useRef, useCallback } from "react";
import Link from "next/link";
import {
  fetchAgents,
  fetchStrategies,
  fetchSessions,
  createSession,
  fetchSession,
  renameSession,
  deleteSession,
  executeAgentInSession,
  executeTeamInSession,
  type AgentInfo,
  type StrategyInfo,
  type SessionSummary,
  type SessionMessage,
} from "@/lib/api";

// ── Types ──────────────────────────────────────────────────────────────

interface ChatMessage {
  id: string;
  role: "user" | "agent" | "team" | "error";
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

// ── Helpers ─────────────────────────────────────────────────────────────

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
  return STATUS_COLORS[status.toLowerCase()] || "#64748b";
}

function tierColor(tier: string): string {
  return TIER_COLORS[tier] || "#64748b";
}

function sessionMessageToChatMessage(msg: SessionMessage): ChatMessage {
  return {
    id: msg.id,
    role: msg.role,
    content: msg.content,
    timestamp: new Date(msg.timestamp).getTime(),
    agentName: msg.agent_name,
    status: msg.status,
    executionTimeMs: msg.execution_time_ms,
    teamOutputs: msg.team_outputs,
    agentsUsed: msg.agents_used,
    strategy: msg.strategy,
  };
}

function relativeTime(dateStr: string): string {
  const diff = Date.now() - new Date(dateStr).getTime();
  const mins = Math.floor(diff / 60000);
  if (mins < 1) return "now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
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

  // Session state
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [refreshKey, setRefreshKey] = useState(0);

  const messagesEndRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLInputElement>(null);

  // Load agents, strategies, and sessions on mount
  useEffect(() => {
    async function init() {
      try {
        const [agentList, strategyList, sessionList] = await Promise.allSettled([
          fetchAgents(),
          fetchStrategies(),
          fetchSessions(),
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
        if (sessionList.status === "fulfilled") {
          setSessions(sessionList.value);
          if (sessionList.value.length > 0) {
            // Select most recent session
            const mostRecent = sessionList.value[0];
            setActiveSessionId(mostRecent.id);
            // Load its messages
            const session = await fetchSession(mostRecent.id);
            setMessages(session.messages.map(sessionMessageToChatMessage));
          } else {
            // Auto-create first session
            const session = await createSession();
            setSessions([{ ...session, message_count: 0, last_agent: null, last_mode: null }]);
            setActiveSessionId(session.id);
            setMessages([]);
          }
        }
      } catch (e) {
        setInitError(`Failed to load agents: ${(e as Error).message}`);
      }
    }
    init();
  }, []);

  // Refresh session list when refreshKey changes
  useEffect(() => {
    if (refreshKey === 0) return;
    fetchSessions()
      .then(setSessions)
      .catch(() => {});
  }, [refreshKey]);

  // Auto-scroll to bottom on new messages
  useEffect(() => {
    messagesEndRef.current?.scrollIntoView({ behavior: "smooth" });
  }, [messages]);

  // ── Session handlers ────────────────────────────────────────────────

  const handleNewSession = useCallback(async () => {
    try {
      const session = await createSession();
      setActiveSessionId(session.id);
      setMessages([]);
      setRefreshKey((k) => k + 1);
    } catch (err) {
      console.error("Failed to create session:", err);
    }
  }, []);

  const handleSelectSession = useCallback(async (id: string) => {
    try {
      setActiveSessionId(id);
      const session = await fetchSession(id);
      setMessages(session.messages.map(sessionMessageToChatMessage));
    } catch (err) {
      console.error("Failed to load session:", err);
    }
  }, []);

  const handleRenameSession = useCallback(async (id: string, name: string) => {
    try {
      await renameSession(id, name);
      setRefreshKey((k) => k + 1);
    } catch (err) {
      console.error("Failed to rename session:", err);
    }
  }, []);

  const handleDeleteSession = useCallback(
    async (id: string) => {
      try {
        await deleteSession(id);
        if (activeSessionId === id) {
          // Select another session or create new
          const remaining = sessions.filter((s) => s.id !== id);
          if (remaining.length > 0) {
            await handleSelectSession(remaining[0].id);
          } else {
            await handleNewSession();
          }
        }
        setRefreshKey((k) => k + 1);
      } catch (err) {
        console.error("Failed to delete session:", err);
      }
    },
    [activeSessionId, sessions, handleSelectSession, handleNewSession]
  );

  // ── Chat submit (session-aware) ────────────────────────────────────

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      const task = input.trim();
      if (!task || loading || !activeSessionId) return;

      // Optimistic UI: show user message immediately
      const optimisticMsg: ChatMessage = {
        id: `opt-${Date.now()}`,
        role: "user",
        content: task,
        timestamp: Date.now(),
      };
      setMessages((prev) => [...prev, optimisticMsg]);
      setInput("");
      setLoading(true);

      try {
        if (mode === "agent") {
          const res = await executeAgentInSession(
            activeSessionId,
            selectedAgent,
            task
          );
          // Replace with server truth (includes both user + agent messages)
          const session = await fetchSession(activeSessionId);
          setMessages(session.messages.map(sessionMessageToChatMessage));
        } else {
          await executeTeamInSession(
            activeSessionId,
            task,
            selectedStrategy
          );
          // Replace with server truth
          const session = await fetchSession(activeSessionId);
          setMessages(session.messages.map(sessionMessageToChatMessage));
        }
        setRefreshKey((k) => k + 1);
      } catch (err) {
        // Keep user message, append error
        const errorMsg: ChatMessage = {
          id: `err-${Date.now()}`,
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
    [input, loading, mode, selectedAgent, selectedStrategy, activeSessionId]
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

      {/* Main content: sidebar + chat */}
      <div className="flex-1 flex overflow-hidden">
        {/* Session sidebar */}
        <SessionSidebar
          sessions={sessions}
          activeSessionId={activeSessionId}
          onSelectSession={handleSelectSession}
          onNewSession={handleNewSession}
          onRenameSession={handleRenameSession}
          onDeleteSession={handleDeleteSession}
        />

        {/* Chat area */}
        <div className="flex-1 flex flex-col min-w-0">
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
                      ? "Send a task to the selected agent. Messages persist across page refreshes."
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
                disabled={loading || !activeSessionId}
                className="flex-1 bg-transparent text-sm text-slate-200 placeholder:text-slate-600 outline-none font-mono"
              />
              <button
                type="submit"
                disabled={loading || !input.trim() || !activeSessionId}
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
      </div>
    </div>
  );
}

// ── Session Sidebar ────────────────────────────────────────────────────

function SessionSidebar({
  sessions,
  activeSessionId,
  onSelectSession,
  onNewSession,
  onRenameSession,
  onDeleteSession,
}: {
  sessions: SessionSummary[];
  activeSessionId: string | null;
  onSelectSession: (id: string) => void;
  onNewSession: () => void;
  onRenameSession: (id: string, name: string) => void;
  onDeleteSession: (id: string) => void;
}) {
  const [editingId, setEditingId] = useState<string | null>(null);
  const [editName, setEditName] = useState("");
  const [confirmDeleteId, setConfirmDeleteId] = useState<string | null>(null);

  const startRename = (id: string, currentName: string) => {
    setEditingId(id);
    setEditName(currentName);
  };

  const commitRename = (id: string) => {
    if (editName.trim()) {
      onRenameSession(id, editName.trim());
    }
    setEditingId(null);
  };

  return (
    <div
      className="w-60 shrink-0 flex flex-col overflow-hidden"
      style={{
        borderRight: "1px solid rgba(0, 240, 255, 0.06)",
        background: "rgba(0, 0, 0, 0.15)",
      }}
    >
      {/* New session button */}
      <button
        onClick={onNewSession}
        className="m-3 px-3 py-2 rounded-lg text-xs font-bold tracking-wider uppercase transition-all hover:opacity-90"
        style={{
          background: "rgba(0, 240, 255, 0.1)",
          border: "1px solid rgba(0, 240, 255, 0.2)",
          color: "#00f0ff",
        }}
      >
        + New Session
      </button>

      {/* Session list */}
      <div className="flex-1 overflow-y-auto px-2 pb-2 space-y-1">
        {sessions.map((s) => {
          const isActive = s.id === activeSessionId;
          const isEditing = s.id === editingId;
          const isConfirmingDelete = s.id === confirmDeleteId;

          return (
            <div
              key={s.id}
              className="group rounded-lg px-3 py-2.5 cursor-pointer transition-all"
              style={{
                background: isActive ? "rgba(0, 240, 255, 0.06)" : "transparent",
                borderLeft: isActive ? "2px solid #00f0ff" : "2px solid transparent",
              }}
              onClick={() => !isEditing && onSelectSession(s.id)}
            >
              {isEditing ? (
                <input
                  autoFocus
                  value={editName}
                  onChange={(e) => setEditName(e.target.value)}
                  onBlur={() => commitRename(s.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter") commitRename(s.id);
                    if (e.key === "Escape") setEditingId(null);
                  }}
                  onClick={(e) => e.stopPropagation()}
                  className="w-full bg-transparent text-xs text-slate-200 font-mono outline-none px-1 py-0.5 rounded"
                  style={{ border: "1px solid rgba(0, 240, 255, 0.3)" }}
                />
              ) : (
                <>
                  <div className="flex items-center justify-between">
                    <span className="text-xs text-slate-300 font-mono truncate flex-1">
                      {s.name}
                    </span>
                    {/* Hover actions */}
                    <div className="hidden group-hover:flex items-center gap-1 ml-1">
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          startRename(s.id, s.name);
                        }}
                        className="text-[9px] text-slate-500 hover:text-slate-300 px-1"
                        title="Rename"
                      >
                        &#9998;
                      </button>
                      <button
                        onClick={(e) => {
                          e.stopPropagation();
                          if (isConfirmingDelete) {
                            onDeleteSession(s.id);
                            setConfirmDeleteId(null);
                          } else {
                            setConfirmDeleteId(s.id);
                            setTimeout(() => setConfirmDeleteId(null), 3000);
                          }
                        }}
                        className="text-[9px] px-1"
                        style={{
                          color: isConfirmingDelete ? "#ff4757" : "#64748b",
                        }}
                        title={isConfirmingDelete ? "Click again to confirm" : "Delete"}
                      >
                        &#10005;
                      </button>
                    </div>
                  </div>
                  <div className="flex items-center gap-2 mt-1">
                    {s.message_count > 0 && (
                      <span
                        className="text-[9px] font-mono px-1 rounded"
                        style={{
                          background: "rgba(0, 240, 255, 0.08)",
                          color: "#64748b",
                        }}
                      >
                        {s.message_count} msg
                      </span>
                    )}
                    {s.last_agent && (
                      <span
                        className="text-[9px] font-mono px-1 rounded truncate"
                        style={{
                          background:
                            s.last_mode === "team"
                              ? "rgba(168, 85, 247, 0.1)"
                              : "rgba(0, 240, 255, 0.06)",
                          color:
                            s.last_mode === "team" ? "#a855f7" : "#00f0ff",
                        }}
                      >
                        {s.last_agent}
                      </span>
                    )}
                    <span className="text-[9px] text-slate-600 font-mono ml-auto">
                      {relativeTime(s.updated_at)}
                    </span>
                  </div>
                </>
              )}
            </div>
          );
        })}
      </div>
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
