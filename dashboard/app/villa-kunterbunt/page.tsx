"use client";

import Link from "next/link";
import { useEffect, useState } from "react";
import { useVillaStore } from "@/lib/villa/store";
import WidgetCanvas from "@/components/villa/WidgetCanvas";
import ChatPanel from "@/components/villa/ChatPanel";
import UndoToast from "@/components/villa/UndoToast";
import DashboardSelector from "@/components/villa/DashboardSelector";

// ── Status Bar ───────────────────────────────────────────────────────────────

function StatusBar() {
  const widgetCount = useVillaStore((s) => s.widgets.length);
  const [savedAt, setSavedAt] = useState<string | null>(null);

  // Track localStorage writes via storage event (fires in other tabs)
  // and a polling approach for same-tab writes
  useEffect(() => {
    const check = () => {
      const raw = localStorage.getItem("villa-layout-v1");
      if (raw) {
        setSavedAt(new Date().toLocaleTimeString([], { hour: "2-digit", minute: "2-digit", second: "2-digit" }));
      }
    };
    // Check after store hydrates + every 2s (matches debounce)
    check();
    const id = setInterval(check, 2000);
    return () => clearInterval(id);
  }, []);

  return (
    <div
      className="fixed bottom-0 left-0 right-0 z-40 flex items-center gap-4 px-4 py-1.5"
      style={{
        background: "rgba(6, 8, 13, 0.85)",
        borderTop: "1px solid rgba(0, 212, 255, 0.06)",
        backdropFilter: "blur(8px)",
      }}
    >
      {/* Widget count */}
      <span className="text-[10px] font-mono text-slate-500">
        {widgetCount} widget{widgetCount !== 1 ? "s" : ""}
      </span>

      <span className="text-slate-700 text-[10px]">|</span>

      {/* Saved indicator */}
      {savedAt && (
        <span className="text-[10px] font-mono text-slate-600 flex items-center gap-1">
          <span
            className="w-1 h-1 rounded-full inline-block"
            style={{ background: "#06d6a0" }}
          />
          Saved {savedAt}
        </span>
      )}
    </div>
  );
}

// ── Page ──────────────────────────────────────────────────────────────────────

export default function VillaKunterbuntPage() {
  const widgetCount = useVillaStore((s) => s.widgets.length);

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          background:
            "linear-gradient(180deg, rgba(0, 240, 255, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-3">
          <Link href="/" className="text-slate-500 hover:text-slate-300 text-xs">
            &larr; Dashboard
          </Link>
          <h1
            className="text-lg font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            Villa Kunterbunt
          </h1>
          <span className="text-slate-500 text-xs tracking-widest uppercase">
            {widgetCount} widget{widgetCount !== 1 && "s"}
          </span>
          <span className="text-slate-700 mx-1">|</span>
          <DashboardSelector />
        </div>
      </header>

      {/* Grid canvas — takes remaining vertical space, adjusts for chat panel */}
      <div className="flex-1 px-6 py-4 pb-8 flex min-h-0">
        <WidgetCanvas />
      </div>

      {/* Chat sidebar — fixed overlay on right */}
      <ChatPanel />

      {/* Undo toast — shows when a widget is removed */}
      <UndoToast />

      {/* Status bar — fixed at bottom */}
      <StatusBar />
    </div>
  );
}
