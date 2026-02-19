"use client";

import PageRankChart from "@/components/viz/PageRankChart";
import { adaptPageRankData } from "@/lib/villa/adapters/page-rank";

interface PageRankWidgetProps {
  data: unknown;
  dimensions: { width: number; height: number };
}

export default function PageRankWidget({ data, dimensions }: PageRankWidgetProps) {
  const entries = adaptPageRankData(data);

  if (entries.length === 0) {
    return (
      <div className="flex items-center justify-center h-full">
        <span className="text-xs text-slate-600 font-mono">No PageRank data</span>
      </div>
    );
  }

  return (
    <div style={{ width: dimensions.width, height: dimensions.height }} className="overflow-hidden">
      <PageRankChart data={entries} />
    </div>
  );
}
