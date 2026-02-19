"use client";

import Treemap from "@/components/viz/Treemap";
import { adaptTreemapData } from "@/lib/villa/adapters/treemap";

interface TreemapWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function TreemapWidget({ data, dimensions }: TreemapWidgetProps) {
  const treeData = adaptTreemapData(data);

  if (!treeData.children || treeData.children.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No hierarchy data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <Treemap data={treeData} />
    </div>
  );
}
