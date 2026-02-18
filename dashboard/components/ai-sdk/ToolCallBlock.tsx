"use client";

import { useState, useMemo } from "react";

// ── Types ──────────────────────────────────────────────────────────────

export interface ToolCallBlockProps {
  toolName: string;
  toolCallId: string;
  args: Record<string, unknown>;
  result?: unknown;
  state: "partial-call" | "call" | "result";
}

// ── Tool color mapping ─────────────────────────────────────────────────

const TOOL_COLORS: Record<string, string> = {
  db_query: "#06b6d4",   // cyan
  memory: "#a855f7",     // purple
};

const DEFAULT_TOOL_COLOR = "#64748b";

function getToolColor(toolName: string): string {
  return TOOL_COLORS[toolName] ?? DEFAULT_TOOL_COLOR;
}

// ── Tool display names ─────────────────────────────────────────────────

const TOOL_LABELS: Record<string, string> = {
  db_query: "Database Query",
  memory: "Memory",
};

function getToolLabel(toolName: string): string {
  return TOOL_LABELS[toolName] ?? toolName;
}

// ── Collapsible threshold ──────────────────────────────────────────────

const COLLAPSE_LINE_THRESHOLD = 10;

// ── Component ──────────────────────────────────────────────────────────

export default function ToolCallBlock({
  toolName,
  toolCallId,
  args,
  result,
  state,
}: ToolCallBlockProps) {
  const [expanded, setExpanded] = useState(false);
  const color = getToolColor(toolName);

  const isLoading = state === "call";
  const hasResult = state === "result" && result !== undefined;

  const inputDisplay = useMemo(() => formatInput(toolName, args), [toolName, args]);

  const resultDisplay = useMemo(() => {
    if (result === undefined || result === null) return null;
    if (typeof result === "string") return result;
    return JSON.stringify(result, null, 2);
  }, [result]);

  const resultLines = resultDisplay?.split("\n") ?? [];
  const isLongResult = resultLines.length > COLLAPSE_LINE_THRESHOLD;
  const visibleResult =
    isLongResult && !expanded
      ? resultLines.slice(0, COLLAPSE_LINE_THRESHOLD).join("\n")
      : resultDisplay;

  return (
    <div
      className="my-2 rounded-lg overflow-hidden transition-all duration-300"
      style={{
        background: "rgba(0, 0, 0, 0.4)",
        border: `1px solid ${color}20`,
        borderLeft: `3px solid ${color}`,
        boxShadow: hasResult ? `0 0 12px ${color}10` : undefined,
      }}
    >
      {/* ── Header ──────────────────────────────────────────────── */}
      <div
        className="flex items-center gap-2 px-3 py-2"
        style={{ borderBottom: `1px solid ${color}10` }}
      >
        <span
          className="text-[10px] font-mono font-bold tracking-wider"
          style={{ color }}
        >
          {getToolIcon(toolName)} {getToolLabel(toolName)}
        </span>

        {/* State badge */}
        <span
          className={`text-[9px] font-mono font-bold uppercase px-1.5 py-0.5 rounded${
            isLoading ? " animate-pulse" : ""
          }`}
          style={{
            background: `${color}15`,
            color,
          }}
        >
          {state === "partial-call"
            ? "preparing"
            : state === "call"
              ? "running"
              : "done"}
        </span>

        {/* Tool call ID (truncated) */}
        <span className="text-[9px] font-mono text-slate-600 ml-auto truncate max-w-32">
          {toolCallId.slice(0, 12)}
        </span>
      </div>

      {/* ── Input ───────────────────────────────────────────────── */}
      <div className="px-3 py-2">
        <ToolInput toolName={toolName} display={inputDisplay} color={color} />
      </div>

      {/* ── Result ──────────────────────────────────────────────── */}
      {hasResult && resultDisplay && (
        <div
          className="px-3 py-2"
          style={{ borderTop: `1px solid ${color}10` }}
        >
          <div className="text-[9px] font-mono font-bold uppercase tracking-wider text-slate-500 mb-1.5">
            Output
          </div>
          <ResultBody
            toolName={toolName}
            result={result}
            visibleResult={visibleResult}
          />

          {/* Collapse toggle for long outputs */}
          {isLongResult && (
            <button
              onClick={() => setExpanded((prev) => !prev)}
              className="mt-2 flex items-center gap-1 text-[10px] font-mono font-bold tracking-wider transition-colors hover:opacity-80"
              style={{ color }}
            >
              <span style={{ fontSize: "8px" }}>
                {expanded ? "\u25B2" : "\u25BC"}
              </span>
              {expanded
                ? "Collapse"
                : `Show all (${resultLines.length} lines)`}
            </button>
          )}
        </div>
      )}

      {/* ── Loading indicator ───────────────────────────────────── */}
      {isLoading && (
        <div className="px-3 pb-2">
          <div
            className="h-[2px] rounded-full overflow-hidden"
            style={{ background: `${color}15` }}
          >
            <div
              className="h-full rounded-full"
              style={{
                width: "40%",
                background: color,
                animation: "toolcall-slide 1.5s ease-in-out infinite",
              }}
            />
          </div>
        </div>
      )}

      {isLoading && (
        <style>{`
          @keyframes toolcall-slide {
            0% { transform: translateX(-100%); opacity: 0.6; }
            50% { transform: translateX(150%); opacity: 1; }
            100% { transform: translateX(-100%); opacity: 0.6; }
          }
        `}</style>
      )}
    </div>
  );
}

// ── Sub-components ─────────────────────────────────────────────────────

function ToolInput({
  toolName,
  display,
  color,
}: {
  toolName: string;
  display: string;
  color: string;
}) {
  if (toolName === "db_query") {
    return <DbQueryInput display={display} color={color} />;
  }
  if (toolName === "memory") {
    return <MemoryInput display={display} color={color} />;
  }
  return (
    <pre className="text-xs font-mono text-slate-400 whitespace-pre-wrap break-words leading-relaxed">
      {display}
    </pre>
  );
}

function DbQueryInput({ display, color }: { display: string; color: string }) {
  return (
    <div className="flex items-start gap-2">
      <span
        className="text-xs font-mono font-bold shrink-0 select-none"
        style={{ color }}
      >
        Q
      </span>
      <pre className="text-xs font-mono text-slate-200 whitespace-pre-wrap break-words leading-relaxed">
        {display}
      </pre>
    </div>
  );
}

function MemoryInput({ display, color }: { display: string; color: string }) {
  return (
    <div className="flex items-start gap-2">
      <span
        className="text-xs font-mono font-bold shrink-0 select-none"
        style={{ color }}
      >
        M
      </span>
      <pre className="text-xs font-mono text-slate-200 whitespace-pre-wrap break-words leading-relaxed">
        {display}
      </pre>
    </div>
  );
}

function ResultBody({
  toolName,
  result,
  visibleResult,
}: {
  toolName: string;
  result: unknown;
  visibleResult: string | null;
}) {
  // For db_query, show count badge if available
  if (toolName === "db_query" && isDbQueryResult(result)) {
    return (
      <div>
        <div className="flex items-center gap-2 mb-1.5">
          <span className="text-[9px] font-mono px-1.5 py-0.5 rounded bg-cyan-900/30 text-cyan-400">
            {result.query_type}
          </span>
          {result.count !== undefined && (
            <span className="text-[9px] font-mono text-slate-500">
              {result.count} result{result.count !== 1 ? "s" : ""}
            </span>
          )}
          {result.note && (
            <span className="text-[9px] font-mono text-slate-600 italic">
              {result.note}
            </span>
          )}
        </div>
        <pre className="text-xs font-mono text-slate-300 whitespace-pre-wrap break-words leading-relaxed">
          {visibleResult}
        </pre>
      </div>
    );
  }

  return (
    <pre className="text-xs font-mono text-slate-300 whitespace-pre-wrap break-words leading-relaxed">
      {visibleResult}
    </pre>
  );
}

// ── Type guards ─────────────────────────────────────────────────────────

interface DbQueryResult {
  query_type: string;
  count?: number;
  note?: string;
  data?: unknown;
  error?: string;
}

function isDbQueryResult(v: unknown): v is DbQueryResult {
  return (
    typeof v === "object" &&
    v !== null &&
    "query_type" in v &&
    typeof (v as DbQueryResult).query_type === "string"
  );
}

// ── Formatting helpers ─────────────────────────────────────────────────

function getToolIcon(toolName: string): string {
  switch (toolName) {
    case "db_query":
      return "\u26A1"; // lightning
    case "memory":
      return "\uD83E\uDDE0"; // brain
    default:
      return "\u2699"; // gear
  }
}

function formatInput(toolName: string, args: Record<string, unknown>): string {
  if (toolName === "db_query") {
    return formatDbQueryInput(args);
  }
  if (toolName === "memory") {
    return formatMemoryInput(args);
  }
  if (Object.keys(args).length === 0) return "{}";
  return JSON.stringify(args, null, 2);
}

function formatDbQueryInput(args: Record<string, unknown>): string {
  const parts: string[] = [];
  if (typeof args.query_type === "string") {
    parts.push(args.query_type);
  }
  if (typeof args.query === "string") {
    parts.push(`"${args.query}"`);
  }
  if (typeof args.limit === "number") {
    parts.push(`limit=${args.limit}`);
  }
  return parts.length > 0 ? parts.join(" ") : JSON.stringify(args, null, 2);
}

function formatMemoryInput(args: Record<string, unknown>): string {
  const parts: string[] = [];
  if (typeof args.action === "string") {
    parts.push(args.action.toUpperCase());
  }
  if (typeof args.content === "string") {
    const truncated =
      args.content.length > 80
        ? args.content.slice(0, 80) + "..."
        : args.content;
    parts.push(`"${truncated}"`);
  }
  if (typeof args.query === "string") {
    parts.push(`search: "${args.query}"`);
  }
  return parts.length > 0 ? parts.join(" ") : JSON.stringify(args, null, 2);
}
