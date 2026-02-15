"use client";

import { useState } from "react";
import type { TemporalPattern } from "@/lib/api";

const CATEGORY_COLORS: Record<string, string> = {
  Churn: "#ff4757",
  Engagement: "#06d6a0",
  ErrorChain: "#ff8a00",
  Funnel: "#00d4ff",
  Unknown: "#64748b",
};

type SortField = "support" | "member_count";

interface Props {
  data: TemporalPattern[];
}

const CATEGORY_EXPLANATIONS: Record<string, string> = {
  Churn: "Users showing signs of disengagement",
  Engagement: "Active user behavior pattern",
  ErrorChain: "Error propagation sequence",
  Funnel: "Conversion/progression path",
  Unknown: "Uncategorized pattern",
};

export default function PatternList({ data }: Props) {
  const [sortBy, setSortBy] = useState<SortField>("support");
  const [expandedId, setExpandedId] = useState<string | null>(null);

  const sorted = [...data].sort((a, b) => b[sortBy] - a[sortBy]);

  return (
    <div className="w-full h-full overflow-y-auto px-3 py-2">
      {/* Sort controls */}
      <div className="flex items-center gap-2 mb-3">
        <span className="text-[10px] text-slate-500 uppercase tracking-widest">
          Sort by
        </span>
        {(["support", "member_count"] as SortField[]).map((field) => (
          <button
            key={field}
            onClick={() => setSortBy(field)}
            className="text-[10px] font-bold tracking-wider uppercase px-2.5 py-1 rounded-lg transition-all"
            style={{
              color: sortBy === field ? "#00f0ff" : "#475569",
              background:
                sortBy === field
                  ? "rgba(0, 240, 255, 0.08)"
                  : "transparent",
              border: `1px solid ${
                sortBy === field
                  ? "rgba(0, 240, 255, 0.2)"
                  : "rgba(71, 85, 105, 0.2)"
              }`,
            }}
          >
            {field === "support" ? "Support" : "Members"}
          </button>
        ))}
      </div>

      {/* Pattern rows */}
      <div className="space-y-2">
        {sorted.map((pattern) => {
          const catColor = CATEGORY_COLORS[pattern.category] || "#64748b";
          const isExpanded = expandedId === pattern.id;
          return (
            <div
              key={pattern.id}
              className="rounded-xl p-3 relative overflow-hidden cursor-pointer"
              style={{
                background:
                  "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
                border: `1px solid ${isExpanded ? catColor + "40" : catColor + "20"}`,
                boxShadow: isExpanded
                  ? `0 0 30px ${catColor}10`
                  : `0 0 20px ${catColor}05`,
                transition: "border-color 0.2s, box-shadow 0.2s",
              }}
              onClick={() =>
                setExpandedId(isExpanded ? null : pattern.id)
              }
            >
              {/* Top accent line */}
              <div
                className="absolute top-0 left-0 w-full h-[1px]"
                style={{
                  background: `linear-gradient(90deg, transparent, ${catColor}40, transparent)`,
                }}
              />

              {/* Header: category badge + stats + chevron */}
              <div className="flex items-center justify-between mb-2">
                <span
                  className="inline-flex items-center px-2 py-0.5 rounded-full text-[10px] font-bold tracking-wider uppercase"
                  style={{
                    background: `${catColor}15`,
                    color: catColor,
                    border: `1px solid ${catColor}25`,
                  }}
                >
                  {pattern.category}
                </span>
                <div className="flex items-center gap-3">
                  <span className="text-[10px] text-slate-500 font-mono">
                    <span className="text-slate-400">
                      {(pattern.support * 100).toFixed(1)}%
                    </span>{" "}
                    support
                  </span>
                  <span className="text-[10px] text-slate-500 font-mono">
                    <span className="text-slate-400">
                      {pattern.member_count.toLocaleString()}
                    </span>{" "}
                    members
                  </span>
                  {/* Chevron indicator */}
                  <span
                    className="text-[10px] text-slate-500 inline-block"
                    style={{
                      transform: isExpanded
                        ? "rotate(180deg)"
                        : "rotate(0deg)",
                      transition: "transform 0.25s ease",
                    }}
                  >
                    ▾
                  </span>
                </div>
              </div>

              {/* Sequence flow */}
              <div className="flex flex-wrap items-center gap-1 mb-2">
                {pattern.sequence.map((step, i) => (
                  <span key={i} className="flex items-center gap-1">
                    {i > 0 && (
                      <span className="text-slate-600 text-[10px]">
                        &rarr;
                      </span>
                    )}
                    <span
                      className="inline-block px-2 py-0.5 rounded text-[10px] font-mono font-medium"
                      style={{
                        background: "rgba(0, 240, 255, 0.06)",
                        color: "#94a3b8",
                        border: "1px solid rgba(0, 240, 255, 0.1)",
                      }}
                    >
                      {step}
                    </span>
                  </span>
                ))}
              </div>

              {/* Footer: duration + description */}
              <div className="flex items-center gap-3">
                <span className="text-[10px] text-slate-600 font-mono">
                  avg {formatDuration(pattern.avg_duration_secs)}
                </span>
                {pattern.description && !isExpanded && (
                  <span className="text-[10px] text-slate-500 truncate">
                    {pattern.description}
                  </span>
                )}
              </div>

              {/* Expandable detail panel */}
              <div
                style={{
                  maxHeight: isExpanded ? "500px" : "0px",
                  overflow: "hidden",
                  transition: "max-height 0.3s ease",
                }}
              >
                <div
                  className="mt-3 pt-3"
                  style={{
                    borderTop: `1px solid ${catColor}15`,
                  }}
                >
                  {/* Category explanation */}
                  <div className="mb-3">
                    <span
                      className="text-[10px] font-medium"
                      style={{ color: catColor }}
                    >
                      {CATEGORY_EXPLANATIONS[pattern.category] ||
                        CATEGORY_EXPLANATIONS.Unknown}
                    </span>
                  </div>

                  {/* Detailed sequence with step numbers */}
                  <div className="mb-3">
                    <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold block mb-1.5">
                      Sequence
                    </span>
                    <div className="flex flex-wrap items-center gap-1.5">
                      {pattern.sequence.map((step, i) => (
                        <span
                          key={i}
                          className="flex items-center gap-1"
                        >
                          {i > 0 && (
                            <span
                              className="text-[10px]"
                              style={{ color: catColor + "60" }}
                            >
                              →
                            </span>
                          )}
                          <span
                            className="inline-flex items-center gap-1 px-2 py-1 rounded-md text-[10px] font-mono"
                            style={{
                              background: `${catColor}10`,
                              border: `1px solid ${catColor}20`,
                              color: "#cbd5e1",
                            }}
                          >
                            <span
                              className="text-[8px] font-bold"
                              style={{ color: catColor }}
                            >
                              {i + 1}
                            </span>
                            {step}
                          </span>
                        </span>
                      ))}
                    </div>
                  </div>

                  {/* Statistics */}
                  <div className="grid grid-cols-3 gap-2 mb-3">
                    {/* Support bar */}
                    <div>
                      <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold block mb-1">
                        Support
                      </span>
                      <div className="flex items-center gap-1.5">
                        <div
                          className="h-1.5 rounded-full flex-1"
                          style={{
                            background: "rgba(255,255,255,0.05)",
                          }}
                        >
                          <div
                            className="h-full rounded-full"
                            style={{
                              width: `${Math.min(pattern.support * 100, 100)}%`,
                              background: `linear-gradient(90deg, ${catColor}, ${catColor}80)`,
                              transition: "width 0.3s ease",
                            }}
                          />
                        </div>
                        <span
                          className="text-[11px] font-mono font-bold"
                          style={{ color: catColor }}
                        >
                          {(pattern.support * 100).toFixed(1)}%
                        </span>
                      </div>
                    </div>

                    {/* Member count */}
                    <div>
                      <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold block mb-1">
                        Members
                      </span>
                      <span className="text-[14px] font-mono font-bold text-slate-300">
                        {pattern.member_count.toLocaleString()}
                      </span>
                    </div>

                    {/* Duration */}
                    <div>
                      <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold block mb-1">
                        Avg Duration
                      </span>
                      <span className="text-[14px] font-mono font-bold text-slate-300">
                        {formatDuration(pattern.avg_duration_secs)}
                      </span>
                    </div>
                  </div>

                  {/* Full description */}
                  {pattern.description && (
                    <div>
                      <span className="text-[10px] text-slate-500 uppercase tracking-widest font-bold block mb-1">
                        Description
                      </span>
                      <p className="text-[11px] text-slate-400 leading-relaxed">
                        {pattern.description}
                      </p>
                    </div>
                  )}
                </div>
              </div>
            </div>
          );
        })}
      </div>
    </div>
  );
}

function formatDuration(secs: number): string {
  if (secs < 60) return `${Math.round(secs)}s`;
  if (secs < 3600) return `${Math.round(secs / 60)}m`;
  return `${(secs / 3600).toFixed(1)}h`;
}
