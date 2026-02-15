"use client";

import { useEffect, useRef, useState } from "react";
import { WS_URL, type Stats } from "./api";
import type { Insight, SystemStatus } from "@/components/InsightSidebar";

export type WsStatus = "connecting" | "connected" | "reconnecting" | "disconnected";

interface WsStatsData {
  doc_count: number;
  segment_count: number;
  node_count: number;
  edge_count: number;
  nodes_by_type: Record<string, number>;
  edges_by_type: Record<string, number>;
}

interface WsMessage {
  type: string;
  data: unknown;
}

interface UseWebSocketResult {
  status: WsStatus;
  lastStats: Stats | null;
}

export interface WsCallbacks {
  onInsight?: (insight: Insight) => void;
  onInsightResolved?: (insightId: string) => void;
  onSystemStatus?: (status: SystemStatus) => void;
}

export function useWebSocket(
  onStats: (stats: Stats) => void,
  enabled = true,
  callbacks?: WsCallbacks,
): UseWebSocketResult {
  const [status, setStatus] = useState<WsStatus>("disconnected");
  const [lastStats, setLastStats] = useState<Stats | null>(null);
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const onStatsRef = useRef(onStats);
  const enabledRef = useRef(enabled);
  const callbacksRef = useRef(callbacks);

  useEffect(() => {
    onStatsRef.current = onStats;
  }, [onStats]);

  useEffect(() => {
    enabledRef.current = enabled;
  }, [enabled]);

  useEffect(() => {
    callbacksRef.current = callbacks;
  }, [callbacks]);

  useEffect(() => {
    if (!enabled) {
      // Disconnect
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      const ws = wsRef.current;
      if (ws) {
        ws.onclose = null;
        ws.close();
        wsRef.current = null;
      }
      // Defer setState to avoid synchronous setState in effect body
      // (React Compiler rule: react-hooks/set-state-in-effect).
      queueMicrotask(() => setStatus("disconnected"));
      return;
    }

    // Connect
    retryRef.current = 0;

    function doConnect() {
      if (!enabledRef.current) return;
      if (wsRef.current?.readyState === WebSocket.OPEN) return;

      setStatus("connecting");
      const ws = new WebSocket(WS_URL);
      wsRef.current = ws;

      ws.onopen = () => {
        setStatus("connected");
        retryRef.current = 0;
      };

      ws.onmessage = (event) => {
        try {
          const msg: WsMessage = JSON.parse(event.data);
          if (msg.type === "stats") {
            const d = msg.data as WsStatsData;
            const stats: Stats = {
              doc_count: d.doc_count,
              segment_count: d.segment_count,
              segment_ids: [],
              node_count: d.node_count,
              edge_count: d.edge_count,
              nodes_by_type: d.nodes_by_type,
              edges_by_type: d.edges_by_type,
            };
            setLastStats(stats);
            onStatsRef.current(stats);
          } else if (msg.type === "insight") {
            callbacksRef.current?.onInsight?.(msg.data as Insight);
          } else if (msg.type === "insight_resolved") {
            const resolved = msg.data as { insight_id: string };
            callbacksRef.current?.onInsightResolved?.(resolved.insight_id);
          } else if (msg.type === "system_status") {
            callbacksRef.current?.onSystemStatus?.(msg.data as SystemStatus);
          }
        } catch {
          // Ignore malformed messages.
        }
      };

      ws.onclose = () => {
        wsRef.current = null;
        if (!enabledRef.current) {
          setStatus("disconnected");
          return;
        }
        const delay = Math.min(1000 * Math.pow(2, retryRef.current), 30000);
        retryRef.current++;
        setStatus("reconnecting");
        retryTimerRef.current = setTimeout(doConnect, delay);
      };

      ws.onerror = () => {
        // onclose will fire after onerror, which handles reconnect.
      };
    }

    doConnect();

    return () => {
      if (retryTimerRef.current) {
        clearTimeout(retryTimerRef.current);
        retryTimerRef.current = null;
      }
      const ws = wsRef.current;
      if (ws) {
        ws.onclose = null;
        ws.close();
        wsRef.current = null;
      }
    };
  }, [enabled]);

  return { status, lastStats };
}
