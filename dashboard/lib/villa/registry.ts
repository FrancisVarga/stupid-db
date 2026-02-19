// Villa widget registry — maps WidgetType to component metadata.

import type { ComponentType } from "react";
import dynamic from "next/dynamic";
import type { WidgetType } from "./types";

// eslint-disable-next-line @typescript-eslint/no-explicit-any
type AnyProps = any;

interface WidgetRegistryEntry {
  component: ComponentType<AnyProps>;
  defaultSize: { w: number; h: number };
  minSize: { w: number; h: number };
}

const StatsCardWidget = dynamic(
  () => import("@/components/villa/widgets/StatsCardWidget"),
  { ssr: false },
);

const TimeSeriesChartWidget = dynamic(
  () => import("@/components/villa/widgets/TimeSeriesChartWidget"),
  { ssr: false },
);

const DataTableWidget = dynamic(
  () => import("@/components/villa/widgets/DataTableWidget"),
  { ssr: false },
);

const ForceGraphWidget = dynamic(
  () => import("@/components/villa/widgets/ForceGraphWidget"),
  { ssr: false },
);

const BarChartWidget = dynamic(
  () => import("@/components/villa/widgets/BarChartWidget"),
  { ssr: false },
);

const ScatterPlotWidget = dynamic(
  () => import("@/components/villa/widgets/ScatterPlotWidget"),
  { ssr: false },
);

const HeatmapWidget = dynamic(
  () => import("@/components/villa/widgets/HeatmapWidget"),
  { ssr: false },
);

const SankeyWidget = dynamic(
  () => import("@/components/villa/widgets/SankeyWidget"),
  { ssr: false },
);

const TreemapWidget = dynamic(
  () => import("@/components/villa/widgets/TreemapWidget"),
  { ssr: false },
);

const AnomalyChartWidget = dynamic(
  () => import("@/components/villa/widgets/AnomalyChartWidget"),
  { ssr: false },
);

const TrendChartWidget = dynamic(
  () => import("@/components/villa/widgets/TrendChartWidget"),
  { ssr: false },
);

const PageRankWidget = dynamic(
  () => import("@/components/villa/widgets/PageRankWidget"),
  { ssr: false },
);

const DegreeChartWidget = dynamic(
  () => import("@/components/villa/widgets/DegreeChartWidget"),
  { ssr: false },
);

const registry: Record<WidgetType, WidgetRegistryEntry> = {
  "stats-card": {
    component: StatsCardWidget,
    defaultSize: { w: 3, h: 2 },
    minSize: { w: 2, h: 2 },
  },
  "time-series": {
    component: TimeSeriesChartWidget,
    defaultSize: { w: 6, h: 4 },
    minSize: { w: 4, h: 3 },
  },
  "data-table": {
    component: DataTableWidget,
    defaultSize: { w: 6, h: 4 },
    minSize: { w: 4, h: 3 },
  },
  "force-graph": {
    component: ForceGraphWidget,
    defaultSize: { w: 6, h: 6 },
    minSize: { w: 4, h: 4 },
  },
  "bar-chart": {
    component: BarChartWidget,
    defaultSize: { w: 6, h: 4 },
    minSize: { w: 3, h: 3 },
  },
  "scatter-plot": {
    component: ScatterPlotWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 4 },
  },
  "heatmap": {
    component: HeatmapWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 4 },
  },
  "sankey": {
    component: SankeyWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 4 },
  },
  "treemap": {
    component: TreemapWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 3 },
  },
  "anomaly-chart": {
    component: AnomalyChartWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 3 },
  },
  "trend-chart": {
    component: TrendChartWidget,
    defaultSize: { w: 6, h: 5 },
    minSize: { w: 4, h: 3 },
  },
  "page-rank": {
    component: PageRankWidget,
    defaultSize: { w: 6, h: 4 },
    minSize: { w: 4, h: 3 },
  },
  "degree-chart": {
    component: DegreeChartWidget,
    defaultSize: { w: 6, h: 4 },
    minSize: { w: 4, h: 3 },
  },
};

/** Get the React component for a given widget type. */
export function getWidgetComponent(type: WidgetType): ComponentType<AnyProps> {
  return registry[type].component;
}

/** Get the default grid size for a widget type. */
export function getDefaultSize(type: WidgetType): { w: number; h: number } {
  return registry[type].defaultSize;
}

/** Get the minimum grid size for a widget type. */
export function getMinSize(type: WidgetType): { w: number; h: number } {
  return registry[type].minSize;
}
