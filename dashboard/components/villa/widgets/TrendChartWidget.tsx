"use client";

import TrendChart from "@/components/viz/TrendChart";
import { adaptTrendData } from "@/lib/villa/adapters/trend-chart";

interface TrendChartWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function TrendChartWidget({ data, dimensions }: TrendChartWidgetProps) {
  const entries = adaptTrendData(data);

  if (entries.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No trend data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }} className="overflow-hidden">
      <TrendChart data={entries} />
    </div>
  );
}
