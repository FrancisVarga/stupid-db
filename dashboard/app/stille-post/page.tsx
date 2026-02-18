"use client";

import { useState } from "react";
import Link from "next/link";
import AgentManager from "@/components/stille-post/AgentManager";
import DataSourceManager from "@/components/stille-post/DataSourceManager";
import ScheduleManager from "@/components/stille-post/ScheduleManager";
import ReportViewer from "@/components/stille-post/ReportViewer";
import RunHistory from "@/components/stille-post/RunHistory";
import DeliveryConfig from "@/components/stille-post/DeliveryConfig";
import PipelineBuilder from "@/components/stille-post/PipelineBuilder";

const TABS = [
  "Agents",
  "Pipelines",
  "Schedules",
  "Reports",
  "Data Sources",
  "Runs",
] as const;

type Tab = (typeof TABS)[number];

export default function StillePostPage() {
  const [activeTab, setActiveTab] = useState<Tab>("Agents");
  const [refreshKey, setRefreshKey] = useState(0);

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div
            className="w-[1px] h-4"
            style={{ background: "rgba(0, 240, 255, 0.12)" }}
          />
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            stupid-db{" "}
            <span style={{ color: "#06d6a0" }}>/ Stille Post</span>
          </h1>
        </div>
      </header>

      {/* Tab navigation */}
      <nav
        className="px-6 flex items-center gap-1 shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.06)",
          background: "rgba(6, 8, 13, 0.5)",
        }}
      >
        {TABS.map((tab) => {
          const isActive = activeTab === tab;
          return (
            <button
              key={tab}
              onClick={() => setActiveTab(tab)}
              className="relative px-4 py-2.5 text-xs font-bold uppercase tracking-wider transition-colors"
              style={{
                color: isActive ? "#00f0ff" : "#475569",
              }}
            >
              {tab}
              {isActive && (
                <div
                  className="absolute bottom-0 left-0 w-full h-[2px]"
                  style={{
                    background:
                      "linear-gradient(90deg, transparent, #00f0ff, transparent)",
                  }}
                />
              )}
            </button>
          );
        })}
      </nav>

      {/* Tab content */}
      <div className="flex-1 overflow-y-auto px-8 py-6">
        <TabContent tab={activeTab} refreshKey={refreshKey} />
      </div>
    </div>
  );
}

function TabContent({ tab, refreshKey }: { tab: Tab; refreshKey: number }) {
  switch (tab) {
    case "Agents":
      return <AgentManager refreshKey={refreshKey} />;
    case "Pipelines":
      return <PipelineBuilder refreshKey={refreshKey} />;
    case "Schedules":
      return (
        <>
          <ScheduleManager />
          <div className="mt-8">
            <DeliveryConfig />
          </div>
        </>
      );
    case "Reports":
      return <ReportViewer />;
    case "Data Sources":
      return <DataSourceManager />;
    case "Runs":
      return <RunHistory />;
  }
}
