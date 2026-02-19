"use client";

import SankeyDiagram from "@/components/viz/SankeyDiagram";
import { adaptSankeyData } from "@/lib/villa/adapters/sankey";

interface SankeyWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function SankeyWidget({ data, dimensions }: SankeyWidgetProps) {
  const { nodes, links } = adaptSankeyData(data);

  if (nodes.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No flow data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <SankeyDiagram nodes={nodes} links={links} />
    </div>
  );
}
