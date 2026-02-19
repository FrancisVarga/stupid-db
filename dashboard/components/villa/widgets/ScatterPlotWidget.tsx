"use client";

import ScatterPlot from "@/components/viz/ScatterPlot";
import { adaptScatterPlotData } from "@/lib/villa/adapters/scatter-plot";

interface ScatterPlotWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function ScatterPlotWidget({ data, dimensions }: ScatterPlotWidgetProps) {
  const points = adaptScatterPlotData(data);

  if (points.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <ScatterPlot data={points} />
    </div>
  );
}
