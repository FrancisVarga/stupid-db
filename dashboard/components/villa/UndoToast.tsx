"use client";

import { useEffect, useState } from "react";
import { useVillaStore } from "@/lib/villa/store";
import { useReducedMotion } from "@/lib/villa/useReducedMotion";

const AUTO_DISMISS_MS = 5_000;

export default function UndoToast() {
  const lastRemoved = useVillaStore((s) => s._lastRemoved);
  const undoRemove = useVillaStore((s) => s.undoRemove);
  const [visible, setVisible] = useState(false);
  const reducedMotion = useReducedMotion();

  useEffect(() => {
    if (!lastRemoved) {
      setVisible(false);
      return;
    }
    // With reduced motion, show immediately (no transition needed)
    let frameId: number | undefined;
    if (reducedMotion) {
      setVisible(true);
    } else {
      frameId = requestAnimationFrame(() => setVisible(true));
    }
    const timer = setTimeout(() => {
      setVisible(false);
      // Clear _lastRemoved after fade-out finishes (instant if reduced motion)
      setTimeout(() => useVillaStore.setState({ _lastRemoved: null }), reducedMotion ? 0 : 300);
    }, AUTO_DISMISS_MS);
    return () => {
      if (frameId !== undefined) cancelAnimationFrame(frameId);
      clearTimeout(timer);
    };
  }, [lastRemoved, reducedMotion]);

  if (!lastRemoved) return null;

  const handleUndo = () => {
    undoRemove();
    setVisible(false);
  };

  return (
    <div
      className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 flex items-center gap-3 px-4 py-2.5 rounded-lg text-sm"
      style={{
        background: "rgba(15, 18, 28, 0.95)",
        border: "1px solid rgba(0, 212, 255, 0.2)",
        boxShadow: "0 4px 24px rgba(0, 0, 0, 0.5), 0 0 12px rgba(0, 212, 255, 0.08)",
        opacity: visible ? 1 : 0,
        transition: reducedMotion ? "none" : "opacity 0.3s ease",
        pointerEvents: visible ? "auto" : "none",
      }}
    >
      <span className="text-slate-300">
        Removed <span className="font-semibold text-slate-100">{lastRemoved.widget.title}</span>
      </span>
      <button
        onClick={handleUndo}
        className="font-bold text-xs uppercase tracking-wider px-2 py-0.5 rounded transition-colors"
        style={{ color: "#00d4ff", background: "rgba(0, 212, 255, 0.1)" }}
      >
        Undo
      </button>
    </div>
  );
}
