"use client";

import AnomalyChart from "@/components/viz/AnomalyChart";
import { adaptAnomalyData } from "@/lib/villa/adapters/anomaly-chart";

interface AnomalyChartWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function AnomalyChartWidget({ data, dimensions }: AnomalyChartWidgetProps) {
  const entries = adaptAnomalyData(data);

  if (entries.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No anomalies detected</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }} className="overflow-hidden">
      <AnomalyChart data={entries} />
    </div>
  );
}
