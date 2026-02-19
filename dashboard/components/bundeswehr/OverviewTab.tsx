"use client";

import { useEffect, useRef, useState } from "react";
import * as d3 from "d3";
import { fetchBundeswehrOverview, type BundeswehrOverview } from "@/lib/api";

const TIER_COLORS: Record<string, string> = {
  architect: "#ff4757",
  lead: "#a855f7",
  specialist: "#06d6a0",
};

const TIER_LABELS: Record<string, string> = {
  architect: "Architect",
  lead: "Lead",
  specialist: "Specialist",
};

export default function OverviewTab() {
  const [data, setData] = useState<BundeswehrOverview | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setLoading(true);
    setError(null);
    fetchBundeswehrOverview()
      .then(setData)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load overview"))
      .finally(() => setLoading(false));
  }, []);

  if (loading) {
    return (
      <div className="space-y-4">
        <div className="grid grid-cols-5 gap-3">
          {Array.from({ length: 5 }).map((_, i) => (
            <div
              key={i}
              className="h-20 rounded-xl animate-pulse"
              style={{ background: "rgba(0, 240, 255, 0.03)" }}
            />
          ))}
        </div>
        <div className="grid grid-cols-3 gap-4">
          {Array.from({ length: 3 }).map((_, i) => (
            <div
              key={i}
              className="h-48 rounded-xl animate-pulse"
              style={{ background: "rgba(0, 240, 255, 0.03)" }}
            />
          ))}
        </div>
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

  if (!data) return null;

  const totalTokens = data.top_agents.reduce((sum, a) => sum + a.executions, 0);

  return (
    <div className="space-y-5">
      {/* Stat cards */}
      <div className="grid grid-cols-5 gap-3">
        <StatCard label="Total Agents" value={String(data.total_agents)} accent="#00f0ff" />
        <StatCard label="Executions" value={formatNumber(data.total_executions)} accent="#a855f7" />
        <StatCard
          label="Avg Error Rate"
          value={`${(data.avg_error_rate * 100).toFixed(1)}%`}
          accent={data.avg_error_rate > 0.1 ? "#ff4757" : "#06d6a0"}
        />
        <StatCard
          label="Active Groups"
          value={String(
            Object.values(data.agents_by_tier).reduce((s, n) => s + (n > 0 ? 1 : 0), 0)
          )}
          accent="#fbbf24"
        />
        <StatCard label="Total Tokens" value={formatNumber(totalTokens)} accent="#f472b6" />
      </div>

      {/* Charts row */}
      <div className="grid grid-cols-3 gap-4">
        {/* Tier distribution donut */}
        <Panel title="Agent Distribution" accent="#fbbf24">
          <TierDonut tiers={data.agents_by_tier} total={data.total_agents} />
        </Panel>

        {/* Top agents */}
        <Panel title="Top 5 Agents" accent="#00f0ff">
          {data.top_agents.length === 0 ? (
            <EmptyState text="No execution data" />
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr className="text-slate-500 text-[10px] uppercase tracking-wider">
                  <th className="text-left pb-2 font-medium">Agent</th>
                  <th className="text-right pb-2 font-medium">Executions</th>
                </tr>
              </thead>
              <tbody>
                {data.top_agents.slice(0, 5).map((agent, i) => (
                  <tr key={agent.name} className="border-t" style={{ borderColor: "rgba(0, 240, 255, 0.06)" }}>
                    <td className="py-1.5 text-slate-300 font-mono">
                      <span className="text-slate-600 mr-2">{i + 1}.</span>
                      {agent.name}
                    </td>
                    <td className="py-1.5 text-right font-mono" style={{ color: "#00f0ff" }}>
                      {formatNumber(agent.executions)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Panel>

        {/* Needs attention */}
        <Panel title="Needs Attention" accent="#ff4757">
          {data.worst_agents.length === 0 ? (
            <EmptyState text="All agents healthy" />
          ) : (
            <table className="w-full text-xs">
              <thead>
                <tr className="text-slate-500 text-[10px] uppercase tracking-wider">
                  <th className="text-left pb-2 font-medium">Agent</th>
                  <th className="text-right pb-2 font-medium">Error Rate</th>
                </tr>
              </thead>
              <tbody>
                {data.worst_agents.map((agent) => (
                  <tr key={agent.name} className="border-t" style={{ borderColor: "rgba(255, 71, 87, 0.08)" }}>
                    <td className="py-1.5 text-slate-300 font-mono">{agent.name}</td>
                    <td className="py-1.5 text-right font-mono" style={{ color: "#ff4757" }}>
                      {(agent.error_rate * 100).toFixed(1)}%
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </Panel>
      </div>
    </div>
  );
}

// ── D3 Donut Chart ─────────────────────────────────────────────

function TierDonut({
  tiers,
  total,
}: {
  tiers: { architect: number; lead: number; specialist: number };
  total: number;
}) {
  const svgRef = useRef<SVGSVGElement>(null);

  useEffect(() => {
    if (!svgRef.current) return;

    const entries = Object.entries(tiers)
      .filter(([, v]) => v > 0)
      .map(([key, value]) => ({
        key,
        value,
        color: TIER_COLORS[key] ?? "#64748b",
        label: TIER_LABELS[key] ?? key,
      }));

    if (entries.length === 0) return;

    const width = 200;
    const height = 200;
    const radius = Math.min(width, height) / 2;
    const innerRadius = radius * 0.6;

    const svg = d3.select(svgRef.current);
    svg.selectAll("*").remove();

    const g = svg
      .attr("viewBox", `0 0 ${width} ${height}`)
      .append("g")
      .attr("transform", `translate(${width / 2},${height / 2})`);

    const pie = d3
      .pie<(typeof entries)[0]>()
      .value((d) => d.value)
      .sort(null)
      .padAngle(0.03);

    const arc = d3
      .arc<d3.PieArcDatum<(typeof entries)[0]>>()
      .innerRadius(innerRadius)
      .outerRadius(radius)
      .cornerRadius(3);

    g.selectAll("path")
      .data(pie(entries))
      .join("path")
      .attr("d", arc)
      .attr("fill", (d) => d.data.color)
      .attr("opacity", 0.85);

    // Center label
    g.append("text")
      .attr("text-anchor", "middle")
      .attr("dy", "-0.1em")
      .attr("fill", "#e2e8f0")
      .attr("font-size", "24px")
      .attr("font-weight", "bold")
      .attr("font-family", "monospace")
      .text(String(total));

    g.append("text")
      .attr("text-anchor", "middle")
      .attr("dy", "1.4em")
      .attr("fill", "#64748b")
      .attr("font-size", "9px")
      .attr("letter-spacing", "0.1em")
      .text("AGENTS");
  }, [tiers, total]);

  const entries = Object.entries(tiers).filter(([, v]) => v > 0);

  if (entries.length === 0) {
    return <EmptyState text="No agents" />;
  }

  return (
    <div className="flex flex-col items-center gap-3">
      <svg ref={svgRef} className="w-36 h-36" />
      <div className="flex items-center gap-3">
        {entries.map(([key, value]) => (
          <div key={key} className="flex items-center gap-1.5">
            <span
              className="w-2 h-2 rounded-full"
              style={{ background: TIER_COLORS[key] ?? "#64748b" }}
            />
            <span className="text-[10px] text-slate-400 font-mono">
              {TIER_LABELS[key] ?? key} ({value})
            </span>
          </div>
        ))}
      </div>
    </div>
  );
}

// ── Reusable components ────────────────────────────────────────

function StatCard({ label, value, accent }: { label: string; value: string; accent: string }) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accent}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">{label}</div>
      <div className="text-2xl font-bold font-mono mt-1" style={{ color: accent }}>
        {value}
      </div>
    </div>
  );
}

function Panel({
  title,
  accent,
  children,
}: {
  title: string;
  accent: string;
  children: React.ReactNode;
}) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${accent}20`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}40, transparent)` }}
      />
      <div
        className="text-[10px] font-bold uppercase tracking-[0.15em] mb-3"
        style={{ color: accent }}
      >
        {title}
      </div>
      {children}
    </div>
  );
}

function EmptyState({ text }: { text: string }) {
  return (
    <div className="py-6 text-center">
      <div className="text-slate-600 text-xs font-mono">{text}</div>
    </div>
  );
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}
