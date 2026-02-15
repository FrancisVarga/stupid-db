"use client";

import { useEffect, useState, useMemo } from "react";
import Link from "next/link";
import {
  fetchCatalog,
  type Catalog,
  type CatalogEntry,
  type EdgeSummary,
  type ExternalSource,
} from "@/lib/api-catalog";
import {
  fetchStats,
  fetchPatterns,
  fetchAnomalies,
  fetchCommunities,
  fetchTrends,
  type Stats,
  type TemporalPattern,
  type AnomalyEntry,
  type CommunityEntry,
  type TrendEntry,
} from "@/lib/api";

/* ── Entity colour map (shared with explore/page) ── */
const ENTITY_COLORS: Record<string, string> = {
  Member: "#00d4ff",
  Device: "#00ff88",
  Platform: "#ff8a00",
  Currency: "#ffe600",
  VipGroup: "#c084fc",
  Affiliate: "#ff6eb4",
  Game: "#06d6a0",
  Error: "#ff4757",
  Popup: "#9d4edd",
  Provider: "#2ec4b6",
};

type CatalogTab = "entities" | "schema" | "compute" | "segments";

const TABS: { key: CatalogTab; label: string }[] = [
  { key: "entities", label: "Entities" },
  { key: "schema", label: "Schema" },
  { key: "compute", label: "Compute" },
  { key: "segments", label: "Segments" },
];

/* ── Formatters ── */
function fmtNum(n: number): string {
  return n.toLocaleString();
}

/* ══════════════════════════════════════════════════════════════════
   Entities Tab
   ══════════════════════════════════════════════════════════════════ */

function EntitiesTab({
  catalog,
  search,
}: {
  catalog: Catalog;
  search: string;
}) {
  const q = search.toLowerCase();

  const filteredEntities = useMemo(
    () =>
      catalog.entity_types.filter(
        (e) =>
          e.entity_type.toLowerCase().includes(q) ||
          e.sample_keys.some((k) => k.toLowerCase().includes(q)),
      ),
    [catalog.entity_types, q],
  );

  const filteredEdges = useMemo(
    () =>
      catalog.edge_types.filter(
        (e) =>
          e.edge_type.toLowerCase().includes(q) ||
          e.source_types.some((t) => t.toLowerCase().includes(q)) ||
          e.target_types.some((t) => t.toLowerCase().includes(q)),
      ),
    [catalog.edge_types, q],
  );

  return (
    <div className="space-y-6">
      {/* Summary cards */}
      <div className="grid grid-cols-4 gap-4">
        <StatCard label="Total Nodes" value={fmtNum(catalog.total_nodes)} accent="#00d4ff" />
        <StatCard label="Total Edges" value={fmtNum(catalog.total_edges)} accent="#00ff88" />
        <StatCard label="Entity Types" value={catalog.entity_types.length} accent="#c084fc" />
        <StatCard label="Edge Types" value={catalog.edge_types.length} accent="#ff8a00" />
      </div>

      {/* Entity-Edge Relationship Graph */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(0, 240, 255, 0.02)",
          border: "1px solid rgba(0, 240, 255, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Entity Relationship Graph
        </h3>
        <CatalogForceGraph entities={catalog.entity_types} edges={catalog.edge_types} />
      </div>

      {/* Entity types table */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
      >
        <div className="px-4 py-3" style={{ background: "rgba(0, 240, 255, 0.04)" }}>
          <h3 className="text-sm font-semibold text-slate-300 tracking-wide uppercase">
            Entity Types ({filteredEntities.length})
          </h3>
        </div>
        <table className="w-full text-sm">
          <thead>
            <tr className="text-slate-500 text-xs uppercase tracking-wider">
              <th className="text-left px-4 py-2">Type</th>
              <th className="text-right px-4 py-2">Node Count</th>
              <th className="text-left px-4 py-2">Sample Keys</th>
            </tr>
          </thead>
          <tbody>
            {filteredEntities.map((e) => (
              <tr
                key={e.entity_type}
                className="border-t border-white/5 hover:bg-white/[0.02] transition-colors"
              >
                <td className="px-4 py-2.5">
                  <span
                    className="inline-flex items-center gap-2 font-medium"
                    style={{ color: ENTITY_COLORS[e.entity_type] || "#94a3b8" }}
                  >
                    <span
                      className="w-2 h-2 rounded-full"
                      style={{
                        background: ENTITY_COLORS[e.entity_type] || "#94a3b8",
                      }}
                    />
                    {e.entity_type}
                  </span>
                </td>
                <td className="px-4 py-2.5 text-right font-mono text-slate-300">
                  {fmtNum(e.node_count)}
                </td>
                <td className="px-4 py-2.5 text-slate-500 text-xs">
                  {e.sample_keys.slice(0, 3).join(", ")}
                  {e.sample_keys.length > 3 && " ..."}
                </td>
              </tr>
            ))}
            {filteredEntities.length === 0 && (
              <tr>
                <td colSpan={3} className="px-4 py-6 text-center text-slate-500">
                  No entity types match &quot;{search}&quot;
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>

      {/* Edge types table */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
      >
        <div className="px-4 py-3" style={{ background: "rgba(0, 255, 136, 0.04)" }}>
          <h3 className="text-sm font-semibold text-slate-300 tracking-wide uppercase">
            Edge Types ({filteredEdges.length})
          </h3>
        </div>
        <table className="w-full text-sm">
          <thead>
            <tr className="text-slate-500 text-xs uppercase tracking-wider">
              <th className="text-left px-4 py-2">Edge Type</th>
              <th className="text-right px-4 py-2">Count</th>
              <th className="text-left px-4 py-2">Source → Target</th>
            </tr>
          </thead>
          <tbody>
            {filteredEdges.map((e) => (
              <tr
                key={e.edge_type}
                className="border-t border-white/5 hover:bg-white/[0.02] transition-colors"
              >
                <td className="px-4 py-2.5 font-medium text-slate-200">{e.edge_type}</td>
                <td className="px-4 py-2.5 text-right font-mono text-slate-300">
                  {fmtNum(e.count)}
                </td>
                <td className="px-4 py-2.5 text-xs">
                  {e.source_types.map((s) => (
                    <span key={s} style={{ color: ENTITY_COLORS[s] || "#94a3b8" }}>
                      {s}
                    </span>
                  ))}
                  <span className="text-slate-600 mx-1">→</span>
                  {e.target_types.map((t) => (
                    <span key={t} style={{ color: ENTITY_COLORS[t] || "#94a3b8" }}>
                      {t}
                    </span>
                  ))}
                </td>
              </tr>
            ))}
            {filteredEdges.length === 0 && (
              <tr>
                <td colSpan={3} className="px-4 py-6 text-center text-slate-500">
                  No edge types match &quot;{search}&quot;
                </td>
              </tr>
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}

/* ── Catalog Force Graph (schema-level: entity types as nodes, edge types as links) ── */

function CatalogForceGraph({
  entities,
  edges,
}: {
  entities: CatalogEntry[];
  edges: EdgeSummary[];
}) {
  const svgRef = useState<SVGSVGElement | null>(null);

  useEffect(() => {
    const svg = svgRef[0];
    if (!svg || typeof window === "undefined") return;

    // Dynamically import d3 to avoid SSR issues
    import("d3").then((d3) => {
      const width = svg.clientWidth || 600;
      const height = 360;

      svg.setAttribute("viewBox", `0 0 ${width} ${height}`);

      // Clear previous render
      d3.select(svg).selectAll("*").remove();

      const g = d3.select(svg).append("g");

      // Build schema graph: entity types as nodes, edge types as links
      const nodes = entities.map((e) => ({
        id: e.entity_type,
        count: e.node_count,
        color: ENTITY_COLORS[e.entity_type] || "#94a3b8",
      }));

      // Deduplicate links (group edge types by source-target pair)
      const linkMap = new Map<string, { source: string; target: string; labels: string[]; totalCount: number }>();
      for (const edge of edges) {
        for (const src of edge.source_types) {
          for (const tgt of edge.target_types) {
            const key = `${src}→${tgt}`;
            const existing = linkMap.get(key);
            if (existing) {
              existing.labels.push(edge.edge_type);
              existing.totalCount += edge.count;
            } else {
              linkMap.set(key, {
                source: src,
                target: tgt,
                labels: [edge.edge_type],
                totalCount: edge.count,
              });
            }
          }
        }
      }
      const links = Array.from(linkMap.values());

      // Scale node radius by count
      const maxCount = Math.max(...nodes.map((n) => n.count), 1);
      const rScale = d3.scaleSqrt().domain([0, maxCount]).range([12, 36]);

      const simulation = d3
        .forceSimulation(nodes as d3.SimulationNodeDatum[])
        .force(
          "link",
          d3
            .forceLink(links as d3.SimulationLinkDatum<d3.SimulationNodeDatum>[])
            .id((d: d3.SimulationNodeDatum) => (d as (typeof nodes)[0]).id)
            .distance(120),
        )
        .force("charge", d3.forceManyBody().strength(-300))
        .force("center", d3.forceCenter(width / 2, height / 2))
        .force("collision", d3.forceCollide().radius((d) => rScale((d as (typeof nodes)[0]).count) + 8));

      // Arrow marker
      g.append("defs")
        .append("marker")
        .attr("id", "arrow")
        .attr("viewBox", "0 -5 10 10")
        .attr("refX", 20)
        .attr("refY", 0)
        .attr("markerWidth", 6)
        .attr("markerHeight", 6)
        .attr("orient", "auto")
        .append("path")
        .attr("d", "M0,-5L10,0L0,5")
        .attr("fill", "#475569");

      const linkG = g
        .selectAll(".link")
        .data(links)
        .enter()
        .append("g")
        .attr("class", "link");

      const linkLines = linkG
        .append("line")
        .attr("stroke", "#334155")
        .attr("stroke-width", 1.5)
        .attr("marker-end", "url(#arrow)");

      const linkLabels = linkG
        .append("text")
        .text((d) => d.labels.join(", "))
        .attr("font-size", 9)
        .attr("fill", "#64748b")
        .attr("text-anchor", "middle")
        .attr("dy", -4);

      const nodeG = g
        .selectAll(".node")
        .data(nodes)
        .enter()
        .append("g")
        .attr("class", "node")
        .attr("cursor", "grab")
        .call(
          d3
            .drag<SVGGElement, (typeof nodes)[0]>()
            .on("start", (event, d) => {
              if (!event.active) simulation.alphaTarget(0.3).restart();
              (d as d3.SimulationNodeDatum).fx = (d as d3.SimulationNodeDatum).x;
              (d as d3.SimulationNodeDatum).fy = (d as d3.SimulationNodeDatum).y;
            })
            .on("drag", (event, d) => {
              (d as d3.SimulationNodeDatum).fx = event.x;
              (d as d3.SimulationNodeDatum).fy = event.y;
            })
            .on("end", (event, d) => {
              if (!event.active) simulation.alphaTarget(0);
              (d as d3.SimulationNodeDatum).fx = null;
              (d as d3.SimulationNodeDatum).fy = null;
            }),
        );

      nodeG
        .append("circle")
        .attr("r", (d) => rScale(d.count))
        .attr("fill", (d) => d.color + "20")
        .attr("stroke", (d) => d.color)
        .attr("stroke-width", 2);

      nodeG
        .append("text")
        .text((d) => d.id)
        .attr("text-anchor", "middle")
        .attr("dy", -4)
        .attr("font-size", 11)
        .attr("font-weight", 600)
        .attr("fill", (d) => d.color);

      nodeG
        .append("text")
        .text((d) => fmtNum(d.count))
        .attr("text-anchor", "middle")
        .attr("dy", 10)
        .attr("font-size", 9)
        .attr("fill", "#94a3b8");

      simulation.on("tick", () => {
        linkLines
          .attr("x1", (d) => ((d.source as d3.SimulationNodeDatum).x ?? 0))
          .attr("y1", (d) => ((d.source as d3.SimulationNodeDatum).y ?? 0))
          .attr("x2", (d) => ((d.target as d3.SimulationNodeDatum).x ?? 0))
          .attr("y2", (d) => ((d.target as d3.SimulationNodeDatum).y ?? 0));

        linkLabels
          .attr("x", (d) => (((d.source as d3.SimulationNodeDatum).x ?? 0) + ((d.target as d3.SimulationNodeDatum).x ?? 0)) / 2)
          .attr("y", (d) => (((d.source as d3.SimulationNodeDatum).y ?? 0) + ((d.target as d3.SimulationNodeDatum).y ?? 0)) / 2);

        nodeG.attr("transform", (d) => `translate(${(d as d3.SimulationNodeDatum).x},${(d as d3.SimulationNodeDatum).y})`);
      });

      return () => {
        simulation.stop();
      };
    });
  }, [entities, edges, svgRef]);

  return (
    <svg
      ref={(el) => {
        if (el && el !== svgRef[0]) {
          svgRef[1](el);
        }
      }}
      className="w-full"
      style={{ height: 360 }}
    />
  );
}

/* ══════════════════════════════════════════════════════════════════
   Schema Tab
   ══════════════════════════════════════════════════════════════════ */

function SchemaTab({
  sources,
  search,
}: {
  sources: ExternalSource[];
  search: string;
}) {
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const q = search.toLowerCase();

  const toggle = (key: string) =>
    setExpanded((prev) => ({ ...prev, [key]: !prev[key] }));

  if (!sources || sources.length === 0) {
    return (
      <div className="text-center py-12">
        <div className="text-slate-500 text-sm mb-2">No external SQL sources configured</div>
        <div className="text-slate-600 text-xs">
          Connect an Athena, Trino, or PostgreSQL source to see schema metadata here.
        </div>
      </div>
    );
  }

  return (
    <div className="space-y-4">
      {sources.map((src) => {
        const srcKey = src.connection_id;
        const matchesSearch =
          !q ||
          src.name.toLowerCase().includes(q) ||
          src.kind.toLowerCase().includes(q) ||
          src.databases.some(
            (db) =>
              db.name.toLowerCase().includes(q) ||
              db.tables.some(
                (t) =>
                  t.name.toLowerCase().includes(q) ||
                  t.columns.some((c) => c.name.toLowerCase().includes(q)),
              ),
          );

        if (!matchesSearch) return null;

        return (
          <div
            key={srcKey}
            className="rounded-xl overflow-hidden"
            style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
          >
            <button
              className="w-full px-4 py-3 flex items-center justify-between text-left hover:bg-white/[0.02] transition-colors"
              style={{ background: "rgba(0, 240, 255, 0.04)" }}
              onClick={() => toggle(srcKey)}
            >
              <div className="flex items-center gap-3">
                <span className="text-sm font-semibold text-slate-200">{src.name}</span>
                <span
                  className="px-2 py-0.5 rounded text-xs font-medium"
                  style={{
                    background: "rgba(0, 240, 255, 0.1)",
                    color: "#00f0ff",
                  }}
                >
                  {src.kind}
                </span>
                <span className="text-xs text-slate-500">{src.connection_id}</span>
              </div>
              <span className="text-slate-500 text-xs">
                {src.databases.reduce((acc, db) => acc + db.tables.length, 0)} tables
                {expanded[srcKey] ? " ▾" : " ▸"}
              </span>
            </button>

            {expanded[srcKey] && (
              <div className="px-4 pb-4 space-y-3">
                {src.databases.map((db) => (
                  <div key={db.name}>
                    <button
                      className="flex items-center gap-2 text-xs font-medium text-slate-400 mb-2 hover:text-slate-300"
                      onClick={() => toggle(`${srcKey}:${db.name}`)}
                    >
                      <span>{expanded[`${srcKey}:${db.name}`] ? "▾" : "▸"}</span>
                      <span className="uppercase tracking-wider">Database: {db.name}</span>
                      <span className="text-slate-600">({db.tables.length} tables)</span>
                    </button>

                    {expanded[`${srcKey}:${db.name}`] &&
                      db.tables.map((tbl) => {
                        const tblMatches =
                          !q ||
                          tbl.name.toLowerCase().includes(q) ||
                          tbl.columns.some((c) => c.name.toLowerCase().includes(q));
                        if (!tblMatches) return null;

                        return (
                          <div
                            key={tbl.name}
                            className="ml-4 mb-2 rounded-lg overflow-hidden"
                            style={{ border: "1px solid rgba(255,255,255,0.04)" }}
                          >
                            <button
                              className="w-full px-3 py-2 flex items-center justify-between text-left hover:bg-white/[0.02]"
                              onClick={() => toggle(`${srcKey}:${db.name}:${tbl.name}`)}
                            >
                              <span className="text-sm text-slate-300 font-medium">
                                {tbl.name}
                              </span>
                              <span className="text-xs text-slate-500">
                                {tbl.columns.length} cols
                                {expanded[`${srcKey}:${db.name}:${tbl.name}`] ? " ▾" : " ▸"}
                              </span>
                            </button>

                            {expanded[`${srcKey}:${db.name}:${tbl.name}`] && (
                              <div className="px-3 pb-2">
                                <table className="w-full text-xs">
                                  <thead>
                                    <tr className="text-slate-600 uppercase tracking-wider">
                                      <th className="text-left py-1">Column</th>
                                      <th className="text-left py-1">Type</th>
                                    </tr>
                                  </thead>
                                  <tbody>
                                    {tbl.columns
                                      .filter(
                                        (c) => !q || c.name.toLowerCase().includes(q),
                                      )
                                      .map((col) => (
                                        <tr
                                          key={col.name}
                                          className="border-t border-white/[0.03]"
                                        >
                                          <td className="py-1 text-slate-300">{col.name}</td>
                                          <td className="py-1">
                                            <span
                                              className="px-1.5 py-0.5 rounded text-[10px] font-medium"
                                              style={{
                                                background: "rgba(192, 132, 252, 0.1)",
                                                color: "#c084fc",
                                              }}
                                            >
                                              {col.data_type}
                                            </span>
                                          </td>
                                        </tr>
                                      ))}
                                  </tbody>
                                </table>
                              </div>
                            )}
                          </div>
                        );
                      })}
                  </div>
                ))}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════════
   Compute Tab
   ══════════════════════════════════════════════════════════════════ */

function ComputeTab({ search }: { search: string }) {
  const [patterns, setPatterns] = useState<TemporalPattern[]>([]);
  const [anomalies, setAnomalies] = useState<AnomalyEntry[]>([]);
  const [communities, setCommunities] = useState<CommunityEntry[]>([]);
  const [trends, setTrends] = useState<TrendEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    Promise.all([
      fetchPatterns().catch(() => [] as TemporalPattern[]),
      fetchAnomalies(200).catch(() => [] as AnomalyEntry[]),
      fetchCommunities().catch(() => [] as CommunityEntry[]),
      fetchTrends().catch(() => [] as TrendEntry[]),
    ])
      .then(([p, a, c, t]) => {
        setPatterns(p);
        setAnomalies(a);
        setCommunities(c);
        setTrends(t);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-slate-500 text-sm animate-pulse">Loading compute data...</div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-center py-12 text-red-400 text-sm">Error: {error}</div>
    );
  }

  const q = search.toLowerCase();

  // Pattern categories
  const patternsByCategory = patterns.reduce<Record<string, number>>((acc, p) => {
    acc[p.category] = (acc[p.category] || 0) + 1;
    return acc;
  }, {});

  // Anomaly severity breakdown
  const anomalyBreakdown = {
    critical: anomalies.filter((a) => a.score >= 0.7).length,
    anomalous: anomalies.filter((a) => a.score >= 0.5 && a.score < 0.7).length,
    mild: anomalies.filter((a) => a.score >= 0.3 && a.score < 0.5).length,
    normal: anomalies.filter((a) => a.score < 0.3).length,
  };

  return (
    <div className="space-y-6">
      {/* Summary cards */}
      <div className="grid grid-cols-4 gap-4">
        <StatCard label="Patterns" value={patterns.length} accent="#06d6a0" />
        <StatCard
          label="Anomalies"
          value={anomalies.filter((a) => a.is_anomalous).length}
          accent="#ff4757"
        />
        <StatCard label="Communities" value={communities.length} accent="#c084fc" />
        <StatCard label="Trend Alerts" value={trends.length} accent="#ff8a00" />
      </div>

      {/* Pattern categories */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(6, 214, 160, 0.02)",
          border: "1px solid rgba(6, 214, 160, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Patterns by Category
        </h3>
        <div className="grid grid-cols-5 gap-3">
          {Object.entries(patternsByCategory)
            .sort(([, a], [, b]) => b - a)
            .filter(([cat]) => !q || cat.toLowerCase().includes(q))
            .map(([category, count]) => {
              const colors: Record<string, string> = {
                Churn: "#ff4757",
                Engagement: "#06d6a0",
                ErrorChain: "#ff8a00",
                Funnel: "#00d4ff",
                Unknown: "#64748b",
              };
              return (
                <div
                  key={category}
                  className="rounded-lg p-3 text-center"
                  style={{
                    background: `${colors[category] || "#64748b"}10`,
                    border: `1px solid ${colors[category] || "#64748b"}30`,
                  }}
                >
                  <div
                    className="text-xl font-bold"
                    style={{ color: colors[category] || "#64748b" }}
                  >
                    {count}
                  </div>
                  <div className="text-xs text-slate-400 mt-1">{category}</div>
                </div>
              );
            })}
        </div>
      </div>

      {/* Anomaly breakdown */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(255, 71, 87, 0.02)",
          border: "1px solid rgba(255, 71, 87, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Anomaly Severity Distribution
        </h3>
        <div className="flex items-end gap-2 h-32">
          {(
            [
              { key: "critical", color: "#ff4757", label: "Critical" },
              { key: "anomalous", color: "#ff8a00", label: "Anomalous" },
              { key: "mild", color: "#ffe600", label: "Mild" },
              { key: "normal", color: "#00ff88", label: "Normal" },
            ] as const
          ).map(({ key, color, label }) => {
            const count = anomalyBreakdown[key];
            const maxVal = Math.max(...Object.values(anomalyBreakdown), 1);
            const height = (count / maxVal) * 100;
            return (
              <div key={key} className="flex-1 flex flex-col items-center gap-1">
                <span className="text-xs font-mono" style={{ color }}>
                  {count}
                </span>
                <div
                  className="w-full rounded-t"
                  style={{
                    height: `${Math.max(height, 4)}%`,
                    background: `${color}30`,
                    border: `1px solid ${color}50`,
                    borderBottom: "none",
                  }}
                />
                <span className="text-[10px] text-slate-500">{label}</span>
              </div>
            );
          })}
        </div>
      </div>

      {/* Communities */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(192, 132, 252, 0.02)",
          border: "1px solid rgba(192, 132, 252, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Top Communities by Size
        </h3>
        <div className="space-y-1.5">
          {communities
            .sort((a, b) => b.member_count - a.member_count)
            .slice(0, 10)
            .map((c) => {
              const maxMembers = Math.max(...communities.map((x) => x.member_count), 1);
              const pct = (c.member_count / maxMembers) * 100;
              return (
                <div key={c.community_id} className="flex items-center gap-3">
                  <span className="text-xs text-slate-500 w-6 text-right">
                    #{c.community_id}
                  </span>
                  <div className="flex-1 h-5 rounded overflow-hidden bg-white/[0.03]">
                    <div
                      className="h-full rounded"
                      style={{
                        width: `${pct}%`,
                        background: "rgba(192, 132, 252, 0.3)",
                        border: "1px solid rgba(192, 132, 252, 0.4)",
                      }}
                    />
                  </div>
                  <span className="text-xs font-mono text-slate-400 w-16 text-right">
                    {fmtNum(c.member_count)}
                  </span>
                </div>
              );
            })}
        </div>
      </div>

      {/* Trends */}
      {trends.length > 0 && (
        <div
          className="rounded-xl p-5"
          style={{
            background: "rgba(255, 138, 0, 0.02)",
            border: "1px solid rgba(255, 138, 0, 0.08)",
          }}
        >
          <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
            Active Trend Alerts ({trends.length})
          </h3>
          <div className="space-y-2">
            {trends
              .filter((t) => !q || t.metric.toLowerCase().includes(q))
              .map((t, i) => (
                <div
                  key={i}
                  className="flex items-center gap-3 px-3 py-2 rounded-lg"
                  style={{ background: "rgba(255, 138, 0, 0.05)" }}
                >
                  <span
                    className="text-lg"
                    style={{ color: t.direction === "up" ? "#ff4757" : "#00ff88" }}
                  >
                    {t.direction === "up" ? "↑" : "↓"}
                  </span>
                  <div className="flex-1">
                    <span className="text-sm text-slate-200">{t.metric}</span>
                  </div>
                  <span className="text-xs font-mono text-slate-400">
                    {t.current_value.toFixed(1)} vs {t.baseline_mean.toFixed(1)} baseline
                  </span>
                  <span
                    className="text-xs font-bold"
                    style={{ color: "#ff8a00" }}
                  >
                    {(t.magnitude * 100).toFixed(0)}%
                  </span>
                </div>
              ))}
          </div>
        </div>
      )}
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════════
   Segments Tab
   ══════════════════════════════════════════════════════════════════ */

function SegmentsTab({ search }: { search: string }) {
  const [stats, setStats] = useState<Stats | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchStats()
      .then((s) => {
        setStats(s);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  if (loading) {
    return (
      <div className="flex items-center justify-center py-12">
        <div className="text-slate-500 text-sm animate-pulse">Loading segment data...</div>
      </div>
    );
  }

  if (error || !stats) {
    return (
      <div className="text-center py-12 text-red-400 text-sm">
        {error || "No stats available"}
      </div>
    );
  }

  const q = search.toLowerCase();

  // Filter segment IDs
  const filteredSegments = stats.segment_ids.filter(
    (s) => !q || s.toLowerCase().includes(q),
  );

  return (
    <div className="space-y-6">
      {/* Summary cards */}
      <div className="grid grid-cols-4 gap-4">
        <StatCard label="Segments" value={stats.segment_count} accent="#00d4ff" />
        <StatCard label="Documents" value={fmtNum(stats.doc_count)} accent="#06d6a0" />
        <StatCard label="Nodes" value={fmtNum(stats.node_count)} accent="#c084fc" />
        <StatCard label="Edges" value={fmtNum(stats.edge_count)} accent="#ff8a00" />
      </div>

      {/* Segment Timeline */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(0, 240, 255, 0.02)",
          border: "1px solid rgba(0, 240, 255, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Segment Timeline
        </h3>
        {stats.segment_ids.length > 0 ? (
          <div className="relative">
            {/* Timeline bar */}
            <div className="h-10 rounded-lg overflow-hidden bg-white/[0.03] flex">
              {stats.segment_ids.map((seg, i) => (
                <div
                  key={seg}
                  className="flex-1 border-r border-white/[0.03] hover:bg-[#00f0ff15] transition-colors group relative"
                  title={seg}
                >
                  <div
                    className="absolute bottom-0 left-0 right-0"
                    style={{
                      height: "100%",
                      background:
                        i === stats.segment_ids.length - 1
                          ? "rgba(0, 240, 255, 0.15)"
                          : "rgba(0, 240, 255, 0.05)",
                    }}
                  />
                </div>
              ))}
            </div>
            {/* Timeline labels */}
            <div className="flex justify-between mt-2">
              <span className="text-xs text-slate-500">
                {stats.segment_ids[0]}
              </span>
              <span className="text-xs text-slate-400 font-medium">
                {stats.segment_ids[stats.segment_ids.length - 1]}
                <span className="ml-1 text-[#00f0ff]">(latest)</span>
              </span>
            </div>
          </div>
        ) : (
          <div className="text-center py-4 text-slate-500 text-sm">
            No segments loaded
          </div>
        )}
      </div>

      {/* Entity distribution */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(192, 132, 252, 0.02)",
          border: "1px solid rgba(192, 132, 252, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Nodes by Entity Type
        </h3>
        <div className="space-y-1.5">
          {Object.entries(stats.nodes_by_type)
            .sort(([, a], [, b]) => b - a)
            .filter(([type]) => !q || type.toLowerCase().includes(q))
            .map(([type, count]) => {
              const maxCount = Math.max(...Object.values(stats.nodes_by_type), 1);
              const pct = (count / maxCount) * 100;
              return (
                <div key={type} className="flex items-center gap-3">
                  <span
                    className="text-xs w-20 text-right font-medium"
                    style={{ color: ENTITY_COLORS[type] || "#94a3b8" }}
                  >
                    {type}
                  </span>
                  <div className="flex-1 h-5 rounded overflow-hidden bg-white/[0.03]">
                    <div
                      className="h-full rounded"
                      style={{
                        width: `${pct}%`,
                        background: `${ENTITY_COLORS[type] || "#94a3b8"}30`,
                        border: `1px solid ${ENTITY_COLORS[type] || "#94a3b8"}50`,
                      }}
                    />
                  </div>
                  <span className="text-xs font-mono text-slate-400 w-20 text-right">
                    {fmtNum(count)}
                  </span>
                </div>
              );
            })}
        </div>
      </div>

      {/* Edges by type */}
      <div
        className="rounded-xl p-5"
        style={{
          background: "rgba(0, 255, 136, 0.02)",
          border: "1px solid rgba(0, 255, 136, 0.08)",
        }}
      >
        <h3 className="text-sm font-semibold text-slate-300 mb-4 tracking-wide uppercase">
          Edges by Type
        </h3>
        <div className="space-y-1.5">
          {Object.entries(stats.edges_by_type)
            .sort(([, a], [, b]) => b - a)
            .filter(([type]) => !q || type.toLowerCase().includes(q))
            .map(([type, count]) => {
              const maxCount = Math.max(...Object.values(stats.edges_by_type), 1);
              const pct = (count / maxCount) * 100;
              return (
                <div key={type} className="flex items-center gap-3">
                  <span className="text-xs w-24 text-right font-medium text-slate-300">
                    {type}
                  </span>
                  <div className="flex-1 h-5 rounded overflow-hidden bg-white/[0.03]">
                    <div
                      className="h-full rounded"
                      style={{
                        width: `${pct}%`,
                        background: "rgba(0, 255, 136, 0.25)",
                        border: "1px solid rgba(0, 255, 136, 0.4)",
                      }}
                    />
                  </div>
                  <span className="text-xs font-mono text-slate-400 w-20 text-right">
                    {fmtNum(count)}
                  </span>
                </div>
              );
            })}
        </div>
      </div>

      {/* Segment list */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ border: "1px solid rgba(0, 240, 255, 0.08)" }}
      >
        <div className="px-4 py-3" style={{ background: "rgba(0, 240, 255, 0.04)" }}>
          <h3 className="text-sm font-semibold text-slate-300 tracking-wide uppercase">
            Segment IDs ({filteredSegments.length})
          </h3>
        </div>
        <div className="px-4 py-3 flex flex-wrap gap-2">
          {filteredSegments.map((seg, i) => (
            <span
              key={seg}
              className="px-2 py-1 rounded text-xs font-mono"
              style={{
                background:
                  i === filteredSegments.length - 1
                    ? "rgba(0, 240, 255, 0.1)"
                    : "rgba(255, 255, 255, 0.04)",
                color: i === filteredSegments.length - 1 ? "#00f0ff" : "#94a3b8",
                border: `1px solid ${
                  i === filteredSegments.length - 1
                    ? "rgba(0, 240, 255, 0.2)"
                    : "rgba(255, 255, 255, 0.06)"
                }`,
              }}
            >
              {seg}
            </span>
          ))}
          {filteredSegments.length === 0 && (
            <span className="text-slate-500 text-sm">No segments match &quot;{search}&quot;</span>
          )}
        </div>
      </div>
    </div>
  );
}

/* ── Shared stat card ── */

function StatCard({
  label,
  value,
  accent = "#00f0ff",
}: {
  label: string;
  value: string | number;
  accent?: string;
}) {
  return (
    <div
      className="rounded-xl p-4 relative overflow-hidden"
      style={{
        background: `${accent}06`,
        border: `1px solid ${accent}15`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accent}40, transparent)`,
        }}
      />
      <div className="text-xs text-slate-500 uppercase tracking-wider mb-1">
        {label}
      </div>
      <div className="text-2xl font-bold" style={{ color: accent }}>
        {value}
      </div>
    </div>
  );
}

/* ══════════════════════════════════════════════════════════════════
   Main Catalog Page
   ══════════════════════════════════════════════════════════════════ */

export default function CatalogPage() {
  const [tab, setTab] = useState<CatalogTab>("entities");
  const [search, setSearch] = useState("");
  const [catalog, setCatalog] = useState<Catalog | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    fetchCatalog()
      .then((c) => {
        setCatalog(c);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

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
          <Link href="/" className="hover:opacity-80 transition-opacity">
            <h1
              className="text-lg font-bold tracking-wider"
              style={{ color: "#00f0ff" }}
            >
              stupid-db
            </h1>
          </Link>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            catalog
          </span>
        </div>
        <div className="flex items-center gap-3">
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
          <Link
            href="/explore"
            className="inline-flex items-center gap-1.5 px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-90 transition-opacity"
            style={{
              background: "rgba(0, 240, 255, 0.12)",
              border: "1px solid rgba(0, 240, 255, 0.25)",
              color: "#00f0ff",
            }}
          >
            Open Explorer
          </Link>
        </div>
      </header>

      {/* Tabs + Search */}
      <div
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{ borderBottom: "1px solid rgba(255,255,255,0.04)" }}
      >
        <div className="flex items-center gap-1">
          {TABS.map((t) => (
            <button
              key={t.key}
              onClick={() => {
                setTab(t.key);
                setSearch("");
              }}
              className="px-4 py-1.5 rounded-lg text-xs font-medium tracking-wide transition-all"
              style={{
                background:
                  tab === t.key ? "rgba(0, 240, 255, 0.1)" : "transparent",
                color: tab === t.key ? "#00f0ff" : "#64748b",
                border:
                  tab === t.key
                    ? "1px solid rgba(0, 240, 255, 0.2)"
                    : "1px solid transparent",
              }}
            >
              {t.label}
            </button>
          ))}
        </div>

        <input
          type="text"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          placeholder={`Search ${tab}...`}
          className="px-3 py-1.5 rounded-lg text-xs bg-white/[0.04] border border-white/[0.08] text-slate-200 placeholder:text-slate-600 focus:outline-none focus:border-[#00f0ff30] w-64"
        />
      </div>

      {/* Content */}
      <main className="flex-1 p-6 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center py-20">
            <div className="text-slate-500 text-sm animate-pulse">
              Loading catalog...
            </div>
          </div>
        ) : error ? (
          <div className="flex flex-col items-center justify-center py-20 gap-3">
            <div className="text-red-400 text-sm">
              {error.includes("503") || error.includes("not ready")
                ? "Catalog is still building — the backend is loading data."
                : `Error: ${error}`}
            </div>
            <button
              onClick={() => {
                setError(null);
                setLoading(true);
                fetchCatalog()
                  .then((c) => {
                    setCatalog(c);
                    setLoading(false);
                  })
                  .catch((e) => {
                    setError((e as Error).message);
                    setLoading(false);
                  });
              }}
              className="px-4 py-1.5 rounded-lg text-xs font-medium"
              style={{
                background: "rgba(0, 240, 255, 0.1)",
                border: "1px solid rgba(0, 240, 255, 0.2)",
                color: "#00f0ff",
              }}
            >
              Retry
            </button>
          </div>
        ) : catalog ? (
          <>
            {tab === "entities" && (
              <EntitiesTab catalog={catalog} search={search} />
            )}
            {tab === "schema" && (
              <SchemaTab
                sources={catalog.external_sources || []}
                search={search}
              />
            )}
            {tab === "compute" && <ComputeTab search={search} />}
            {tab === "segments" && <SegmentsTab search={search} />}
          </>
        ) : null}
      </main>
    </div>
  );
}
