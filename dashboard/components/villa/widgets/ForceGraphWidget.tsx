"use client";

import ForceGraph from "@/components/viz/ForceGraph";
import { adaptForceGraphData } from "@/lib/villa/adapters/force-graph";

interface ForceGraphWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function ForceGraphWidget({ data, dimensions }: ForceGraphWidgetProps) {
  const graphData = adaptForceGraphData(data);

  if (graphData.nodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No graph data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <ForceGraph data={graphData} />
    </div>
  );
}
