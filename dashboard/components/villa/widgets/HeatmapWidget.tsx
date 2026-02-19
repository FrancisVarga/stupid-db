"use client";

import CooccurrenceHeatmap from "@/components/viz/CooccurrenceHeatmap";
import { adaptHeatmapData } from "@/lib/villa/adapters/heatmap";

interface HeatmapWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function HeatmapWidget({ data, dimensions }: HeatmapWidgetProps) {
  const heatmapData = adaptHeatmapData(data);

  if (heatmapData.pairs.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No co-occurrence data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <CooccurrenceHeatmap data={heatmapData} />
    </div>
  );
}
