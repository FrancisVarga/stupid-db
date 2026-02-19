"use client";

import { useCallback, useMemo } from "react";
import {
  ResponsiveGridLayout,
  useContainerWidth,
  type Layout,
  type ResponsiveLayouts,
} from "react-grid-layout";
import "react-grid-layout/css/styles.css";
import "react-resizable/css/styles.css";

import { useVillaStore } from "@/lib/villa/store";
import { getWidgetComponent, getMinSize } from "@/lib/villa/registry";
import { useVillaWebSocket } from "@/lib/villa/useVillaWebSocket";
import WidgetShell from "./WidgetShell";

export default function WidgetCanvas() {
  const widgets = useVillaStore((s) => s.widgets);
  const updateAllLayouts = useVillaStore((s) => s.updateAllLayouts);
  const removeWidget = useVillaStore((s) => s.removeWidget);
  const isChatOpen = useVillaStore((s) => s.isChatOpen);

  // Single WS connection shared by all widgets — only active if any widget uses WS
  const hasWsWidgets = widgets.some((w) => w.dataSource.type === "websocket");
  const { status: wsStatus, subscribe: wsSubscribe } = useVillaWebSocket(hasWsWidgets);

  const { width, containerRef, mounted } = useContainerWidth({
    measureBeforeMount: true,
  });

  // Derive react-grid-layout items from Zustand widgets
  const gridLayout = useMemo(
    () =>
      widgets.map((w) => {
        const min = getMinSize(w.type);
        return {
          i: w.id,
          x: w.layout.x,
          y: w.layout.y,
          w: w.layout.w,
          h: w.layout.h,
          minW: w.layout.minW ?? min.w,
          minH: w.layout.minH ?? min.h,
        };
      }),
    [widgets],
  );

  // Sync grid changes back to Zustand
  const onLayoutChange = useCallback(
    (layout: Layout, _layouts: ResponsiveLayouts) => {
      updateAllLayouts(
        layout.map((l) => ({ i: l.i, x: l.x, y: l.y, w: l.w, h: l.h })),
      );
    },
    [updateAllLayouts],
  );

  return (
    <div
      ref={containerRef}
      role="region"
      aria-label="Dashboard widget grid"
      className="flex-1 min-h-0 transition-all"
      style={{ marginRight: isChatOpen ? 400 : 0 }}
    >
      {mounted && widgets.length > 0 && (
        <ResponsiveGridLayout
          width={isChatOpen ? width - 400 : width}
          layouts={{ lg: gridLayout }}
          breakpoints={{ lg: 1200, md: 996, sm: 768, xs: 480 }}
          cols={{ lg: 12, md: 10, sm: 6, xs: 4 }}
          rowHeight={60}
          onLayoutChange={onLayoutChange}
          dragConfig={{ handle: ".villa-drag-handle" }}
        >
          {widgets.map((w) => {
            const WidgetComponent = getWidgetComponent(w.type);
            return (
              <div key={w.id}>
                <WidgetShell config={w} onRemove={removeWidget} wsSubscribe={wsSubscribe} wsStatus={wsStatus}>
                  {(dimensions: { width: number; height: number }, data: unknown) => (
                    <WidgetComponent
                      data={data}
                      dimensions={dimensions}
                    />
                  )}
                </WidgetShell>
              </div>
            );
          })}
        </ResponsiveGridLayout>
      )}

      {/* Empty state */}
      {mounted && widgets.length === 0 && (
        <div className="flex items-center justify-center h-full min-h-[400px]">
          <div className="text-center space-y-3">
            <div
              className="text-4xl opacity-20"
              style={{ filter: "grayscale(1)" }}
            >
              &#x1F3E0;
            </div>
            <p className="text-sm text-slate-500 font-mono">
              No widgets yet.
            </p>
            <p className="text-xs text-slate-600">
              Open the chat and ask for widgets to populate your layout.
            </p>
          </div>
        </div>
      )}
    </div>
  );
}
