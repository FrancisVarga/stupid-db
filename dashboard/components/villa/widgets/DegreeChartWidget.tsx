"use client";

import DegreeChart from "@/components/viz/DegreeChart";
import { adaptDegreeData } from "@/lib/villa/adapters/degree-chart";

interface DegreeChartWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function DegreeChartWidget({ data, dimensions }: DegreeChartWidgetProps) {
  const entries = adaptDegreeData(data);

  if (entries.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No degree data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }} className="overflow-hidden">
      <DegreeChart data={entries} />
    </div>
  );
}
