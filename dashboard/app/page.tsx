"use client";

import { useEffect, useState, useCallback } from "react";
import ChatPanel, { type ChatMessage } from "@/components/chat/ChatPanel";
import ForceGraph from "@/components/viz/ForceGraph";
import PageRankChart from "@/components/viz/PageRankChart";
import DegreeChart from "@/components/viz/DegreeChart";
import {
  fetchStats,
  fetchForceGraph,
  fetchPageRank,
  fetchCommunities,
  fetchDegrees,
  postQuery,
  type Stats,
  type ForceGraphData,
  type PageRankEntry,
  type CommunityEntry,
  type DegreeEntry,
} from "@/lib/api";

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

type VizTab = "graph" | "pagerank" | "communities" | "degrees";

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
    <div className="stat-card rounded-xl p-4 relative overflow-hidden">
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${accent}40, transparent)`,
        }}
      />
      <div className="text-slate-400 text-[10px] uppercase tracking-widest">
        {label}
      </div>
      <div
        className="text-2xl font-bold font-mono mt-1"
        style={{ color: accent }}
      >
        {typeof value === "number" ? value.toLocaleString() : value}
      </div>
    </div>
  );
}

function EntityBadge({ type, count }: { type: string; count: number }) {
  const color = ENTITY_COLORS[type] || "#94a3b8";
  return (
    <span
      className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-[10px] font-medium tracking-wide"
      style={{
        background: `${color}12`,
        color: color,
        border: `1px solid ${color}25`,
      }}
    >
      {type}
      <span className="font-mono font-bold">{count.toLocaleString()}</span>
    </span>
  );
}

let msgIdCounter = 0;
function makeMsg(role: "user" | "system", content: string): ChatMessage {
  return {
    id: `msg-${++msgIdCounter}`,
    role,
    content,
    timestamp: new Date(),
  };
}

export default function Home() {
  const [stats, setStats] = useState<Stats | null>(null);
  const [graphData, setGraphData] = useState<ForceGraphData | null>(null);
  const [pageRankData, setPageRankData] = useState<PageRankEntry[]>([]);
  const [communityData, setCommunityData] = useState<CommunityEntry[]>([]);
  const [degreeData, setDegreeData] = useState<DegreeEntry[]>([]);
  const [messages, setMessages] = useState<ChatMessage[]>([]);
  const [activeTab, setActiveTab] = useState<VizTab>("graph");
  const [error, setError] = useState<string | null>(null);

  // Community map for graph coloring — expand top_nodes from each community summary
  const communityMap =
    activeTab === "communities" && communityData.length > 0
      ? new Map(
          communityData.flatMap((c) =>
            c.top_nodes.map((n) => [n.id, c.community_id] as [string, number])
          )
        )
      : undefined;

  // Initial data fetch
  useEffect(() => {
    const load = async () => {
      try {
        const s = await fetchStats();
        setStats(s);

        const welcome = [
          `Connected to stupid-db.`,
          `${s.doc_count.toLocaleString()} documents across ${Object.keys(s.nodes_by_type).length} entity types.`,
          `${s.node_count.toLocaleString()} nodes, ${s.edge_count.toLocaleString()} edges in the knowledge graph.`,
          ``,
          `Try the visualization tabs on the right, or ask a question below.`,
        ].join("\n");
        setMessages([makeMsg("system", welcome)]);

        const gd = await fetchForceGraph(300);
        setGraphData(gd);
      } catch (e) {
        setError(`Failed to connect: ${(e as Error).message}`);
      }
    };
    load();
  }, []);

  // Load compute data when tab changes
  useEffect(() => {
    if (activeTab === "pagerank" && pageRankData.length === 0) {
      fetchPageRank(50).then(setPageRankData).catch(() => {
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Failed to load PageRank data. Is the compute engine running?"),
        ]);
      });
    }
    if (activeTab === "communities" && communityData.length === 0) {
      fetchCommunities().then(setCommunityData).catch(() => {
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Failed to load community data."),
        ]);
      });
    }
    if (activeTab === "degrees" && degreeData.length === 0) {
      fetchDegrees(50).then(setDegreeData).catch(() => {
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Failed to load degree data."),
        ]);
      });
    }
  }, [activeTab, pageRankData.length, communityData.length, degreeData.length]);

  const handleSend = useCallback(
    (text: string) => {
      setMessages((prev) => [...prev, makeMsg("user", text)]);

      // Local command handling (stub for future /query integration)
      const lower = text.toLowerCase();
      if (lower.includes("stats") || lower.includes("status")) {
        if (stats) {
          const reply = [
            `Documents: ${stats.doc_count.toLocaleString()}`,
            `Nodes: ${stats.node_count.toLocaleString()}`,
            `Edges: ${stats.edge_count.toLocaleString()}`,
            ``,
            `Entity breakdown:`,
            ...Object.entries(stats.nodes_by_type)
              .sort(([, a], [, b]) => b - a)
              .map(([t, c]) => `  ${t}: ${c.toLocaleString()}`),
          ].join("\n");
          setMessages((prev) => [...prev, makeMsg("system", reply)]);
        }
      } else if (lower.includes("pagerank") || lower.includes("rank")) {
        setActiveTab("pagerank");
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Switched to PageRank view. Loading top nodes by influence..."),
        ]);
      } else if (lower.includes("communit")) {
        setActiveTab("communities");
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Switched to Communities view. Nodes are now colored by Louvain cluster."),
        ]);
      } else if (lower.includes("degree")) {
        setActiveTab("degrees");
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Switched to Degrees view. Showing most connected nodes..."),
        ]);
      } else if (lower.includes("graph") || lower.includes("force")) {
        setActiveTab("graph");
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Switched to Force Graph view."),
        ]);
      } else {
        // Send to LLM query endpoint
        setMessages((prev) => [
          ...prev,
          makeMsg("system", "Thinking..."),
        ]);
        postQuery(text)
          .then((res) => {
            const resultText = res.results.length > 0
              ? JSON.stringify(res.results, null, 2)
              : "No results found.";
            setMessages((prev) => {
              // Remove the "Thinking..." message
              const filtered = prev.filter((m) => m.content !== "Thinking...");
              return [
                ...filtered,
                makeMsg("system", resultText),
              ];
            });
          })
          .catch((err) => {
            setMessages((prev) => {
              const filtered = prev.filter((m) => m.content !== "Thinking...");
              return [
                ...filtered,
                makeMsg(
                  "system",
                  `Query failed: ${(err as Error).message}\n\nTry: stats, pagerank, communities, degrees, or graph.`
                ),
              ];
            });
          });
      }
    },
    [stats]
  );

  if (error) {
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
          <p className="text-red-300/70 mt-2 text-sm">{error}</p>
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

  const tabs: { key: VizTab; label: string }[] = [
    { key: "graph", label: "Graph" },
    { key: "pagerank", label: "PageRank" },
    { key: "communities", label: "Communities" },
    { key: "degrees", label: "Degrees" },
  ];

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-2.5 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            stupid-db
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            knowledge engine
          </span>
        </div>
        {stats && (
          <div className="flex items-center gap-2">
            <div className="w-1.5 h-1.5 rounded-full bg-green-400 animate-pulse" />
            <span className="text-slate-500 text-xs font-mono">
              {stats.segment_count} segments
            </span>
          </div>
        )}
      </header>

      {/* Stats bar */}
      {stats && (
        <div
          className="px-6 py-3 shrink-0"
          style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          <div className="grid grid-cols-4 gap-3 mb-2">
            <StatCard label="Documents" value={stats.doc_count} accent="#00f0ff" />
            <StatCard label="Segments" value={stats.segment_count} accent="#06d6a0" />
            <StatCard label="Nodes" value={stats.node_count} accent="#a855f7" />
            <StatCard label="Edges" value={stats.edge_count} accent="#f472b6" />
          </div>
          <div className="flex flex-wrap gap-1.5">
            {Object.entries(stats.nodes_by_type)
              .sort(([, a], [, b]) => b - a)
              .map(([type, count]) => (
                <EntityBadge key={type} type={type} count={count} />
              ))}
          </div>
        </div>
      )}

      {/* Main: Chat (left 60%) + Viz (right 40%) */}
      <div className="flex-1 flex min-h-0">
        {/* Chat panel */}
        <div
          className="flex flex-col"
          style={{
            width: "60%",
            borderRight: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <ChatPanel messages={messages} onSend={handleSend} />
        </div>

        {/* Viz panel */}
        <div className="flex flex-col" style={{ width: "40%" }}>
          {/* Tabs */}
          <div
            className="flex shrink-0"
            style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.08)" }}
          >
            {tabs.map((tab) => (
              <button
                key={tab.key}
                onClick={() => setActiveTab(tab.key)}
                className="flex-1 py-2.5 text-[10px] font-bold tracking-[0.15em] uppercase transition-all"
                style={{
                  color:
                    activeTab === tab.key ? "#00f0ff" : "#475569",
                  background:
                    activeTab === tab.key
                      ? "rgba(0, 240, 255, 0.05)"
                      : "transparent",
                  borderBottom:
                    activeTab === tab.key
                      ? "2px solid #00f0ff"
                      : "2px solid transparent",
                }}
              >
                {tab.label}
              </button>
            ))}
          </div>

          {/* Tab content */}
          <div className="flex-1 min-h-0 relative graph-bg">
            {activeTab === "graph" && (
              graphData ? (
                <ForceGraph data={graphData} />
              ) : (
                <div className="flex items-center justify-center h-full">
                  <div className="text-slate-600 text-sm animate-pulse">
                    Loading graph...
                  </div>
                </div>
              )
            )}
            {activeTab === "pagerank" && (
              pageRankData.length > 0 ? (
                <PageRankChart data={pageRankData} />
              ) : (
                <div className="flex items-center justify-center h-full">
                  <div className="text-slate-600 text-sm animate-pulse">
                    Loading PageRank...
                  </div>
                </div>
              )
            )}
            {activeTab === "communities" && (
              graphData ? (
                <ForceGraph
                  data={graphData}
                  communityMap={communityMap}
                />
              ) : (
                <div className="flex items-center justify-center h-full">
                  <div className="text-slate-600 text-sm animate-pulse">
                    Loading communities...
                  </div>
                </div>
              )
            )}
            {activeTab === "degrees" && (
              degreeData.length > 0 ? (
                <DegreeChart data={degreeData} />
              ) : (
                <div className="flex items-center justify-center h-full">
                  <div className="text-slate-600 text-sm animate-pulse">
                    Loading degrees...
                  </div>
                </div>
              )
            )}
          </div>
        </div>
      </div>
    </div>
  );
}
