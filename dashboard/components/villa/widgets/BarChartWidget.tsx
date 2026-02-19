"use client";

import BarChart from "@/components/viz/BarChart";
import { adaptBarChartData } from "@/lib/villa/adapters/bar-chart";

interface BarChartWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function BarChartWidget({ data, dimensions }: BarChartWidgetProps) {
  const items = adaptBarChartData(data);

  if (items.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }}>
      <BarChart data={items} />
    </div>
  );
}
