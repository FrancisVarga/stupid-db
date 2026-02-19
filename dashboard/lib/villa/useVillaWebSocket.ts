"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import { WS_URL } from "@/lib/api";

// ── Types ───────────────────────────────────────────────────────────────────

export type VillaWsStatus = "connecting" | "connected" | "reconnecting" | "disconnected";

type MessageCallback = (data: unknown) => void;

interface UseVillaWebSocketResult {
  status: VillaWsStatus;
  subscribe: (widgetId: string, messageType: string, callback: MessageCallback) => () => void;
}

// ── Hook ────────────────────────────────────────────────────────────────────

/**
 * Villa-specific WebSocket hook that routes incoming messages to widgets
 * based on their `wsMessageType` from DataSourceConfig.
 *
 * Usage:
 *   const { status, subscribe } = useVillaWebSocket();
 *   useEffect(() => subscribe("widget-1", "stats", (data) => setData(data)), []);
 */
export function useVillaWebSocket(enabled = true): UseVillaWebSocketResult {
  const [status, setStatus] = useState<VillaWsStatus>("disconnected");
  const wsRef = useRef<WebSocket | null>(null);
  const retryRef = useRef(0);
  const retryTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const enabledRef = useRef(enabled);

  // Map: messageType → Set of { widgetId, callback }
  const subscribersRef = useRef<Map<string, Map<string, MessageCallback>>>(new Map());

  useEffect(() => {
    enabledRef.current = enabled;
  }, [enabled]);

  // Subscribe a widget to a specific message type. Returns unsubscribe fn.
  const subscribe = useCallback(
    (widgetId: string, messageType: string, callback: MessageCallback): (() => void) => {
      const subs = subscribersRef.current;
      if (!subs.has(messageType)) {
        subs.set(messageType, new Map());
      }
      subs.get(messageType)!.set(widgetId, callback);

      return () => {
        const typeMap = subs.get(messageType);
        if (typeMap) {
          typeMap.delete(widgetId);
          if (typeMap.size === 0) subs.delete(messageType);
        }
      };
    },
    [],
  );

  useEffect(() => {
    if (!enabled) {
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
      queueMicrotask(() => setStatus("disconnected"));
      return;
    }

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
          const msg = JSON.parse(event.data) as { type: string; data: unknown };
          const typeMap = subscribersRef.current.get(msg.type);
          if (typeMap) {
            for (const cb of typeMap.values()) {
              cb(msg.data);
            }
          }
        } catch {
          // Ignore malformed messages
        }
      };

      ws.onclose = () => {
        wsRef.current = null;
        if (!enabledRef.current) {
          setStatus("disconnected");
          return;
        }
        const delay = Math.min(1000 * Math.pow(2, retryRef.current), 30_000);
        retryRef.current++;
        setStatus("reconnecting");
        retryTimerRef.current = setTimeout(doConnect, delay);
      };

      ws.onerror = () => {
        // onclose fires after onerror — handles reconnect
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

  return { status, subscribe };
}
