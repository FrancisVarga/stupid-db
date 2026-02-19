"use client";

import { Component, useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import type { WidgetConfig, WidgetType } from "@/lib/villa/types";
import type { VillaWsStatus } from "@/lib/villa/useVillaWebSocket";

// ── Props ────────────────────────────────────────────────────────────────────

interface WidgetShellProps {
  config: WidgetConfig;
  children: (dimensions: { width: number; height: number }, data: unknown) => ReactNode;
  onRemove?: (id: string) => void;
  /** Subscribe fn from useVillaWebSocket — enables WS data for widgets. */
  wsSubscribe?: (widgetId: string, messageType: string, callback: (data: unknown) => void) => () => void;
  /** Current WebSocket connection status. */
  wsStatus?: VillaWsStatus;
}

// ── Error Boundary ───────────────────────────────────────────────────────────

interface ErrorBoundaryProps {
  children: ReactNode;
  onRetry: () => void;
}

interface ErrorBoundaryState {
  error: Error | null;
}

export class WidgetErrorBoundary extends Component<ErrorBoundaryProps, ErrorBoundaryState> {
  state: ErrorBoundaryState = { error: null };

  static getDerivedStateFromError(error: Error): ErrorBoundaryState {
    return { error };
  }

  render() {
    if (this.state.error) {
      return (
        <div className="flex flex-col items-center justify-center h-full gap-2 p-4">
          <p className="text-xs text-red-400 font-mono text-center break-words max-w-full">
            {this.state.error.message}
          </p>
          <button
            onClick={() => {
              this.setState({ error: null });
              this.props.onRetry();
            }}
            className="px-3 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-opacity hover:opacity-80"
            style={{
              background: "rgba(255, 71, 87, 0.15)",
              border: "1px solid rgba(255, 71, 87, 0.3)",
              color: "#ff4757",
            }}
          >
            Retry
          </button>
        </div>
      );
    }
    return this.props.children;
  }
}

// ── Type-aware skeleton loaders ──────────────────────────────────────────────

function WidgetSkeleton({ type }: { type: WidgetType }) {
  switch (type) {
    case "stats-card":
      return (
        <div className="absolute inset-0 p-4 grid grid-cols-2 gap-3">
          {[0, 1, 2, 3].map((i) => (
            <div key={i} className="flex flex-col gap-1.5">
              <div
                className="h-2 w-12 rounded villa-skeleton-shimmer"
                style={{ animationDelay: `${i * 120}ms` }}
              />
              <div
                className="h-5 w-16 rounded villa-skeleton-shimmer"
                style={{ animationDelay: `${i * 120 + 60}ms` }}
              />
            </div>
          ))}
        </div>
      );

    case "time-series":
      return (
        <div className="absolute inset-0 p-4 flex flex-col justify-end">
          <svg className="w-full h-3/4 villa-skeleton-shimmer" viewBox="0 0 200 80" preserveAspectRatio="none">
            <path
              d="M0,60 C20,55 40,30 60,40 C80,50 100,20 120,25 C140,30 160,45 180,15 L200,35"
              fill="none"
              stroke="rgba(0,212,255,0.12)"
              strokeWidth="2"
            />
          </svg>
          <div className="flex justify-between mt-2">
            {[0, 1, 2, 3, 4].map((i) => (
              <div key={i} className="h-1.5 w-6 rounded villa-skeleton-shimmer" style={{ animationDelay: `${i * 80}ms` }} />
            ))}
          </div>
        </div>
      );

    case "data-table":
      return (
        <div className="absolute inset-0 p-3 flex flex-col gap-1.5">
          {/* Header row */}
          <div className="flex gap-3 pb-1.5" style={{ borderBottom: "1px solid rgba(0,212,255,0.06)" }}>
            {[0, 1, 2].map((i) => (
              <div key={i} className="h-2 flex-1 rounded villa-skeleton-shimmer" style={{ animationDelay: `${i * 80}ms` }} />
            ))}
          </div>
          {/* Data rows */}
          {[0, 1, 2, 3, 4].map((row) => (
            <div key={row} className="flex gap-3" style={{ opacity: 1 - row * 0.12 }}>
              {[0, 1, 2].map((col) => (
                <div
                  key={col}
                  className="h-2 flex-1 rounded villa-skeleton-shimmer"
                  style={{ animationDelay: `${(row * 3 + col) * 60}ms` }}
                />
              ))}
            </div>
          ))}
        </div>
      );

    case "force-graph":
      return (
        <div className="absolute inset-0 flex items-center justify-center">
          <div className="relative w-20 h-20">
            <div className="absolute top-1/2 left-1/2 w-4 h-4 -mt-2 -ml-2 rounded-full villa-skeleton-shimmer" />
            {[0, 1, 2, 3].map((i) => (
              <div
                key={i}
                className="absolute w-2 h-2 rounded-full villa-skeleton-shimmer"
                style={{
                  top: `${50 + 40 * Math.sin((i * Math.PI) / 2)}%`,
                  left: `${50 + 40 * Math.cos((i * Math.PI) / 2)}%`,
                  transform: "translate(-50%, -50%)",
                  animationDelay: `${i * 200}ms`,
                }}
              />
            ))}
          </div>
        </div>
      );
  }
}

// ── WidgetShell ──────────────────────────────────────────────────────────────

// ── Fallback polling interval for WS widgets when WS is unavailable ─────────

const WS_FALLBACK_POLL_MS = 10_000;

export default function WidgetShell({ config, children, onRemove, wsSubscribe, wsStatus }: WidgetShellProps) {
  const containerRef = useRef<HTMLDivElement>(null);
  const [dimensions, setDimensions] = useState<{ width: number; height: number } | null>(null);
  const [data, setData] = useState<unknown>(undefined);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [retryKey, setRetryKey] = useState(0);
  const failCountRef = useRef(0);
  const [persistentError, setPersistentError] = useState(false);

  const isWsWidget = config.dataSource.type === "websocket";
  const wsConnected = wsStatus === "connected";

  const MAX_RETRIES = 3;
  const BACKOFF_BASE_MS = 1000;

  // ── Fetch data from endpoint (API widgets + WS fallback) ────────────────
  const fetchData = useCallback(async () => {
    if (persistentError) return;
    setLoading(true);
    setError(null);
    try {
      const params = new URLSearchParams(config.dataSource.params ?? {});
      const sep = config.dataSource.endpoint.includes("?") ? "&" : "?";
      const url = params.toString()
        ? `${config.dataSource.endpoint}${sep}${params}`
        : config.dataSource.endpoint;
      const res = await fetch(url);
      if (!res.ok) throw new Error(`${res.status} ${res.statusText}`);
      setData(await res.json());
      failCountRef.current = 0;
    } catch (e) {
      const count = ++failCountRef.current;
      const msg = e instanceof Error ? e.message : "Fetch failed";
      console.error(`[villa] widget "${config.id}" fetch failed (${count}/${MAX_RETRIES}):`, msg);

      if (count >= MAX_RETRIES) {
        setPersistentError(true);
        setError("Widget data unavailable");
      } else {
        setError(msg);
        const delay = BACKOFF_BASE_MS * Math.pow(2, count - 1);
        setTimeout(fetchData, delay);
      }
    } finally {
      setLoading(false);
    }
  }, [config.dataSource, config.id, persistentError]);

  // Manual retry resets the persistent error state
  const handleManualRetry = useCallback(() => {
    failCountRef.current = 0;
    setPersistentError(false);
    setError(null);
    fetchData();
  }, [fetchData]);

  // ── API widget: fetch + optional polling ────────────────────────────────
  useEffect(() => {
    if (isWsWidget) return;
    fetchData();
    if (config.dataSource.refreshInterval) {
      const id = setInterval(fetchData, config.dataSource.refreshInterval);
      return () => clearInterval(id);
    }
  }, [isWsWidget, fetchData, config.dataSource.refreshInterval]);

  // ── WebSocket widget: subscribe via WS, fallback to polling ─────────────
  useEffect(() => {
    if (!isWsWidget) return;

    const messageType = config.dataSource.wsMessageType;

    // If WS is connected and we have a subscribe function + messageType, use WS
    if (wsConnected && wsSubscribe && messageType) {
      // Initial fetch to seed data (WS may not push immediately)
      fetchData();
      // Then subscribe to live updates
      return wsSubscribe(config.id, messageType, (wsData) => {
        setData(wsData);
        setError(null);
        failCountRef.current = 0;
        setPersistentError(false);
      });
    }

    // WS unavailable — fall back to polling the endpoint
    fetchData();
    const interval = config.dataSource.refreshInterval ?? WS_FALLBACK_POLL_MS;
    const id = setInterval(fetchData, interval);
    return () => clearInterval(id);
  }, [isWsWidget, wsConnected, wsSubscribe, config.id, config.dataSource.wsMessageType, config.dataSource.refreshInterval, fetchData]);

  // ── ResizeObserver ───────────────────────────────────────────────────────
  useEffect(() => {
    const el = containerRef.current;
    if (!el) return;
    const ro = new ResizeObserver(([entry]) => {
      const { width, height } = entry.contentRect;
      if (width > 0 && height > 0) setDimensions({ width, height });
    });
    ro.observe(el);
    return () => ro.disconnect();
  }, []);

  // ── Status indicator color ─────────────────────────────────────────────
  // For WS widgets: green=connected, amber=reconnecting, red=error/disconnected
  // For API widgets: green=ok, red=error
  const indicatorColor = error
    ? "#ff4757"
    : isWsWidget
      ? wsStatus === "connected"
        ? "#06d6a0"
        : wsStatus === "reconnecting" || wsStatus === "connecting"
          ? "#ffa502"
          : "#ff6348"
      : "#06d6a0";

  const indicatorGlow = error ? "#ff475740" : indicatorColor + "40";

  // ── Border style based on state ──────────────────────────────────────────
  const borderColor = error
    ? "rgba(255, 71, 87, 0.4)"
    : loading
      ? "rgba(0, 212, 255, 0.3)"
      : "rgba(0, 212, 255, 0.1)";

  return (
    <>
      {/* CSS-only animations — injected once per mount, deduped by browser */}
      <style>{`
        @keyframes villa-shimmer {
          0%, 100% { background: rgba(0, 212, 255, 0.06); }
          50% { background: rgba(0, 212, 255, 0.12); }
        }
        .villa-skeleton-shimmer {
          animation: villa-shimmer 1.8s ease-in-out infinite;
        }
        @keyframes villa-widget-appear {
          from { opacity: 0; transform: scale(0.95); }
          to { opacity: 1; transform: scale(1); }
        }
        .villa-widget-enter {
          animation: villa-widget-appear 200ms ease-out both;
        }
        @media (prefers-reduced-motion: reduce) {
          .villa-skeleton-shimmer { animation: none; background: rgba(0, 212, 255, 0.08); }
          .villa-widget-enter { animation: none; }
        }
      `}</style>
      <div
        className="flex flex-col h-full rounded-xl overflow-hidden villa-widget-enter"
        style={{
          background: "rgba(6, 8, 13, 0.7)",
          border: `1px solid ${borderColor}`,
          transition: "border-color 0.3s ease",
        }}
      >
        {/* Header */}
        <div
          className="villa-drag-handle flex items-center justify-between px-3 py-2 shrink-0 cursor-grab active:cursor-grabbing"
          style={{ borderBottom: "1px solid rgba(0, 212, 255, 0.08)" }}
        >
          <div className="flex items-center gap-2 min-w-0">
            <span
              className="w-1.5 h-1.5 rounded-full shrink-0"
              style={{
                background: indicatorColor,
                boxShadow: `0 0 6px ${indicatorGlow}`,
                transition: "background 0.3s ease, box-shadow 0.3s ease",
              }}
              title={isWsWidget ? `WS: ${wsStatus ?? "disconnected"}` : undefined}
            />
            <span className="text-[11px] font-semibold text-slate-300 truncate">
              {config.title}
            </span>
            {isWsWidget && wsConnected && (
              <span className="text-[9px] text-emerald-500/60 font-mono uppercase tracking-wider">
                live
              </span>
            )}
          </div>
          {onRemove && (
            <button
              onClick={() => onRemove(config.id)}
              className="text-slate-600 hover:text-red-400 transition-colors text-xs leading-none px-1"
              aria-label={`Remove ${config.title}`}
            >
              ✕
            </button>
          )}
        </div>

        {/* Body */}
        <div ref={containerRef} className="flex-1 min-h-0 relative">
          {loading && !data ? (
            <WidgetSkeleton type={config.type} />
          ) : error ? (
            /* Error state — persistent after 3 failures */
            <div className="flex flex-col items-center justify-center h-full gap-2 p-4">
              <p className="text-xs text-red-400 font-mono text-center">{error}</p>
              {persistentError && (
                <p className="text-[10px] text-slate-600 font-mono text-center">
                  Failed after {MAX_RETRIES} attempts
                </p>
              )}
              <button
                onClick={handleManualRetry}
                className="px-3 py-1 rounded text-[10px] font-bold uppercase tracking-wider transition-opacity hover:opacity-80"
                style={{
                  background: "rgba(255, 71, 87, 0.15)",
                  border: "1px solid rgba(255, 71, 87, 0.3)",
                  color: "#ff4757",
                }}
              >
                Retry
              </button>
            </div>
          ) : dimensions ? (
            <WidgetErrorBoundary
              key={retryKey}
              onRetry={() => setRetryKey((k) => k + 1)}
            >
              {children(dimensions, data)}
            </WidgetErrorBoundary>
          ) : null}
        </div>
      </div>
    </>
  );
}
