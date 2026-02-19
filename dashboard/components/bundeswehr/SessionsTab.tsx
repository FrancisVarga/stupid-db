"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { fetchSessions, type SessionSummary } from "@/lib/api";

interface SessionsTabProps {
  agentName: string;
}

export default function SessionsTab({ agentName }: SessionsTabProps) {
  const [sessions, setSessions] = useState<SessionSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    fetchSessions()
      .then((all) => {
        setSessions(all.filter((s) => s.last_agent === agentName));
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to fetch sessions"))
      .finally(() => setLoading(false));
  }, [agentName]);

  if (loading) {
    return (
      <div className="text-slate-500 text-xs font-mono py-4">
        Loading sessions...
      </div>
    );
  }

  if (error) {
    return (
      <div
        className="px-4 py-3 rounded-lg text-xs"
        style={{
          background: "rgba(255, 71, 87, 0.06)",
          border: "1px solid rgba(255, 71, 87, 0.15)",
          color: "#ff4757",
        }}
      >
        {error}
      </div>
    );
  }

  if (sessions.length === 0) {
    return (
      <div className="py-8 text-center">
        <div className="text-slate-500 text-sm mb-1">No sessions found</div>
        <div className="text-slate-600 text-xs font-mono">
          No sessions have used agent &quot;{agentName}&quot; yet
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-2">
      <div className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500 mb-3">
        {sessions.length} session{sessions.length !== 1 ? "s" : ""}
      </div>
      {sessions.map((s) => (
        <Link
          key={s.id}
          href={`/agents?session=${s.id}`}
          className="flex items-center justify-between px-4 py-3 rounded-lg hover:opacity-90 transition-opacity"
          style={{
            background: "rgba(0, 240, 255, 0.04)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
          }}
        >
          <div className="flex items-center gap-3 min-w-0">
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{ background: "#00f0ff" }}
            />
            <div className="min-w-0">
              <div className="text-xs font-medium text-slate-300 truncate">
                {s.name || s.id}
              </div>
              <div className="text-[10px] font-mono text-slate-600 mt-0.5">
                {s.message_count} message{s.message_count !== 1 ? "s" : ""}
              </div>
            </div>
          </div>
          <div className="flex items-center gap-3 shrink-0">
            {s.last_mode && (
              <span
                className="text-[9px] font-bold uppercase px-1.5 py-0.5 rounded"
                style={{
                  background:
                    s.last_mode === "team"
                      ? "rgba(244, 114, 182, 0.12)"
                      : "rgba(0, 240, 255, 0.08)",
                  color: s.last_mode === "team" ? "#f472b6" : "#00f0ff",
                }}
              >
                {s.last_mode}
              </span>
            )}
            <span className="text-[10px] font-mono text-slate-600">
              {formatRelativeTime(s.updated_at)}
            </span>
          </div>
        </Link>
      ))}
    </div>
  );
}

function formatRelativeTime(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  const mins = Math.floor(diff / 60_000);
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hours = Math.floor(mins / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  return `${days}d ago`;
}
