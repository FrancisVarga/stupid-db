"use client";

import type { RenderBlock } from "@/lib/reports";
import BarChart from "@/components/viz/BarChart";
import LineChart from "@/components/viz/LineChart";
import ScatterPlot from "@/components/viz/ScatterPlot";
import ForceGraph from "@/components/viz/ForceGraph";
import SankeyDiagram from "@/components/viz/SankeyDiagram";
import CooccurrenceHeatmap from "@/components/viz/CooccurrenceHeatmap";
import Treemap from "@/components/viz/Treemap";
import DataTable from "@/components/viz/DataTable";
import InsightCard from "@/components/viz/InsightCard";

interface Props {
  block: RenderBlock;
}

/**
 * Maps a RenderBlock spec from the backend to the appropriate D3 visualization component.
 * This is the central dispatch for inline chat visualizations.
 */
export default function RenderBlockView({ block }: Props) {
  switch (block.type) {
    case "bar_chart":
      return (
        <div style={{ height: 250 }}>
          <BarChart
            data={block.data}
            title={block.title}
            orientation={block.config?.orientation}
          />
        </div>
      );

    case "line_chart":
      return (
        <div style={{ height: 250 }}>
          <LineChart
            data={block.data}
            title={block.title}
            showArea={block.config?.showArea}
          />
        </div>
      );

    case "scatter":
      return (
        <div style={{ height: 300 }}>
          <ScatterPlot data={block.data} title={block.title} />
        </div>
      );

    case "force_graph":
      return (
        <div style={{ height: 300 }}>
          <ForceGraph data={block.data} />
        </div>
      );

    case "sankey":
      return (
        <div style={{ height: 300 }}>
          <SankeyDiagram
            nodes={block.data?.nodes || []}
            links={block.data?.links || []}
            title={block.title}
          />
        </div>
      );

    case "heatmap":
      return (
        <div style={{ height: 300 }}>
          <CooccurrenceHeatmap data={block.data} />
        </div>
      );

    case "treemap":
      return (
        <div style={{ height: 300 }}>
          <Treemap data={block.data} title={block.title} />
        </div>
      );

    case "table":
      return (
        <div style={{ maxHeight: 350 }}>
          <DataTable
            data={block.data}
            columns={block.config?.columns}
            title={block.title}
          />
        </div>
      );

    case "summary":
      return (
        <InsightCard
          title={block.title}
          text={
            typeof block.data === "string"
              ? block.data
              : block.data?.text || ""
          }
          metrics={block.data?.metrics}
          severity={block.config?.severity}
        />
      );

    default:
      return (
        <div className="p-3 text-[10px] text-slate-600 font-mono">
          Unknown render type: {block.type}
        </div>
      );
  }
}
