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
  bash_execute: "#06d6a0",
  file_read: "#3b82f6",
  file_write: "#f59e0b",
  graph_query: "#a855f7",
  rule_list: "#06b6d4",
  rule_evaluate: "#ec4899",
};

const DEFAULT_TOOL_COLOR = "#64748b";

function getToolColor(toolName: string): string {
  return TOOL_COLORS[toolName] ?? DEFAULT_TOOL_COLOR;
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

  // Format input display
  const inputDisplay = useMemo(() => {
    if (toolName === "bash_execute") {
      return formatBashInput(args);
    }
    return formatJsonArgs(args);
  }, [toolName, args]);

  // Format result display
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
          ⚙ {toolName}
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
        {toolName === "bash_execute" ? (
          <BashInput display={inputDisplay} color={color} />
        ) : (
          <JsonInput display={inputDisplay} />
        )}
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
          <pre className="text-xs font-mono text-slate-300 whitespace-pre-wrap break-words leading-relaxed">
            {visibleResult}
          </pre>

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
              className="h-full rounded-full animate-pulse"
              style={{
                width: "40%",
                background: color,
                animation: "toolcall-slide 1.5s ease-in-out infinite",
              }}
            />
          </div>
        </div>
      )}

      {/* Inline keyframes for the loading bar */}
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

function BashInput({
  display,
  color,
}: {
  display: string;
  color: string;
}) {
  return (
    <div className="flex items-start gap-2">
      <span
        className="text-xs font-mono font-bold shrink-0 select-none"
        style={{ color }}
      >
        $
      </span>
      <pre className="text-xs font-mono text-slate-200 whitespace-pre-wrap break-words leading-relaxed">
        {display}
      </pre>
    </div>
  );
}

function JsonInput({ display }: { display: string }) {
  return (
    <pre className="text-xs font-mono text-slate-400 whitespace-pre-wrap break-words leading-relaxed">
      {display}
    </pre>
  );
}

// ── Formatting helpers ─────────────────────────────────────────────────

function formatBashInput(args: Record<string, unknown>): string {
  // AI SDK bash tools typically have a "command" field
  if (typeof args.command === "string") {
    return args.command;
  }
  // Fallback: show the first string arg or JSON
  const firstStringValue = Object.values(args).find(
    (v) => typeof v === "string"
  );
  if (typeof firstStringValue === "string") {
    return firstStringValue;
  }
  return JSON.stringify(args, null, 2);
}

function formatJsonArgs(args: Record<string, unknown>): string {
  if (Object.keys(args).length === 0) return "{}";
  return JSON.stringify(args, null, 2);
}
