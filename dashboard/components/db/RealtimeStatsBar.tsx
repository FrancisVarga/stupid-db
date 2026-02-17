"use client";

import { useEffect, useRef, useState, useCallback } from "react";
import type { RealtimeStats } from "@/lib/api-db";

const HISTORY_LEN = 60; // ~2 min at 2s interval

interface DerivedPoint {
  ts: number;
  tps: number;
  cache_hit_pct: number;
  blks_read_s: number;
  blks_hit_s: number;
  tup_fetched_s: number;
  tup_written_s: number;
  active: number;
  waiting: number;
  buf_backend_s: number;
}

export default function RealtimeStatsBar({ db }: { db: string }) {
  const [history, setHistory] = useState<DerivedPoint[]>([]);
  const [connected, setConnected] = useState(false);
  const [paused, setPaused] = useState(false);
  const [lastError, setLastError] = useState<string | null>(null);
  const prevRef = useRef<RealtimeStats | null>(null);
  const esRef = useRef<EventSource | null>(null);
  const pausedRef = useRef(false);

  // Keep ref in sync so the SSE handler reads the latest value
  pausedRef.current = paused;

  const handleMessage = useCallback((event: MessageEvent) => {
    if (pausedRef.current) return;

    let parsed: Record<string, unknown>;
    try {
      parsed = JSON.parse(event.data);
    } catch {
      return;
    }
    if ("error" in parsed) {
      setLastError(parsed.error as string);
      return;
    }
    setLastError(null);
    const curr = parsed as unknown as RealtimeStats;

    const prev = prevRef.current;
    prevRef.current = curr;
    if (!prev) return; // need two samples for deltas

    const dt = (curr.ts - prev.ts) / 1000;
    if (dt <= 0) return;

    const dBlksHit = curr.blks_hit - prev.blks_hit;
    const dBlksRead = curr.blks_read - prev.blks_read;
    const totalBlks = dBlksHit + dBlksRead;

    const point: DerivedPoint = {
      ts: curr.ts,
      tps: (curr.tps - prev.tps) / dt,
      cache_hit_pct: totalBlks > 0 ? (dBlksHit / totalBlks) * 100 : 100,
      blks_read_s: dBlksRead / dt,
      blks_hit_s: dBlksHit / dt,
      tup_fetched_s: (curr.tup_fetched - prev.tup_fetched) / dt,
      tup_written_s:
        ((curr.tup_inserted - prev.tup_inserted) +
          (curr.tup_updated - prev.tup_updated) +
          (curr.tup_deleted - prev.tup_deleted)) / dt,
      active: curr.active_backends,
      waiting: curr.waiting_backends,
      buf_backend_s: (curr.buffers_backend - prev.buffers_backend) / dt,
    };

    setHistory((h) => [...h.slice(-(HISTORY_LEN - 1)), point]);
  }, []);

  useEffect(() => {
    const url = `/api/v1/meta/${encodeURIComponent(db)}/stats/realtime`;
    const es = new EventSource(url);
    esRef.current = es;

    es.onopen = () => setConnected(true);
    es.onmessage = handleMessage;
    es.onerror = () => setConnected(false);

    return () => {
      es.close();
      esRef.current = null;
    };
  }, [db, handleMessage]);

  const latest = history[history.length - 1];

  return (
    <div className="mb-6">
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          <h2 className="text-[10px] font-bold tracking-[0.15em] uppercase text-slate-500">
            Realtime Metrics
          </h2>
          <span className="text-[8px] font-mono text-slate-600 uppercase tracking-wider">
            SSE
          </span>
        </div>
        <div className="flex items-center gap-3">
          <span
            className="w-1.5 h-1.5 rounded-full transition-all"
            style={{
              background: paused ? "#64748b" : connected ? "#06d6a0" : "#ff4757",
              boxShadow: paused
                ? "none"
                : connected
                  ? "0 0 6px rgba(6, 214, 160, 0.5)"
                  : "0 0 6px rgba(255, 71, 87, 0.5)",
            }}
          />
          <button
            onClick={() => setPaused((p) => !p)}
            className="text-[9px] font-mono uppercase tracking-wider transition-colors hover:text-slate-300"
            style={{ color: "#64748b" }}
          >
            {paused ? "Resume" : "Pause"}
          </button>
        </div>
      </div>

      {lastError && (
        <div
          className="mb-3 rounded-lg px-4 py-2 text-[10px] font-mono flex items-center gap-2"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.12)",
            color: "#ff4757",
          }}
        >
          <span className="w-1.5 h-1.5 rounded-full shrink-0" style={{ background: "#ff4757" }} />
          SSE error: {lastError}
        </div>
      )}

      <div className="grid grid-cols-4 gap-3">
        <MetricCard
          label="Active Backends"
          value={latest ? `${latest.active}` : "--"}
          sub={latest?.waiting ? `${latest.waiting} waiting` : undefined}
          accent="#f59e0b"
          history={history.map((p) => p.active)}
          sparkColor="#f59e0b"
        />
        <MetricCard
          label="Transactions/s"
          value={latest ? fmtRate(latest.tps) : "--"}
          accent="#00f0ff"
          history={history.map((p) => p.tps)}
          sparkColor="#00f0ff"
        />
        <MetricCard
          label="Cache Hit %"
          value={latest ? `${latest.cache_hit_pct.toFixed(1)}%` : "--"}
          accent={latest && latest.cache_hit_pct < 95 ? "#ff4757" : "#06d6a0"}
          history={history.map((p) => p.cache_hit_pct)}
          sparkColor="#06d6a0"
          sparkMin={80}
          sparkMax={100}
        />
        <MetricCard
          label="Disk Reads/s"
          value={latest ? fmtRate(latest.blks_read_s) : "--"}
          sub={latest ? `${fmtRate(latest.buf_backend_s)} direct` : undefined}
          accent="#e879f9"
          history={history.map((p) => p.blks_read_s)}
          sparkColor="#e879f9"
        />
        <MetricCard
          label="Tuples Fetched/s"
          value={latest ? fmtRate(latest.tup_fetched_s) : "--"}
          accent="#38bdf8"
          history={history.map((p) => p.tup_fetched_s)}
          sparkColor="#38bdf8"
        />
        <MetricCard
          label="Tuples Written/s"
          value={latest ? fmtRate(latest.tup_written_s) : "--"}
          accent="#fb923c"
          history={history.map((p) => p.tup_written_s)}
          sparkColor="#fb923c"
        />
        <MetricCard
          label="Buffer Hits/s"
          value={latest ? fmtRate(latest.blks_hit_s) : "--"}
          accent="#a78bfa"
          history={history.map((p) => p.blks_hit_s)}
          sparkColor="#a78bfa"
        />
        <MetricCard
          label="Backend I/O/s"
          value={latest ? fmtRate(latest.buf_backend_s) : "--"}
          accent="#64748b"
          history={history.map((p) => p.buf_backend_s)}
          sparkColor="#64748b"
        />
      </div>
    </div>
  );
}

// ── Metric Card with Sparkline ───────────────────────────────────────

function MetricCard({
  label,
  value,
  sub,
  accent,
  history,
  sparkColor,
  sparkMin,
  sparkMax,
}: {
  label: string;
  value: string;
  sub?: string;
  accent: string;
  history: number[];
  sparkColor: string;
  sparkMin?: number;
  sparkMax?: number;
}) {
  return (
    <div
      className="rounded-xl p-3 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, rgba(12, 16, 24, 0.8) 0%, rgba(17, 24, 39, 0.6) 100%)",
        border: "1px solid rgba(0, 240, 255, 0.06)",
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{ background: `linear-gradient(90deg, transparent, ${accent}30, transparent)` }}
      />
      <div className="text-[9px] text-slate-500 uppercase tracking-widest mb-1">{label}</div>
      <div className="flex items-end justify-between gap-2">
        <div>
          <div className="text-lg font-bold font-mono leading-none" style={{ color: accent }}>
            {value}
          </div>
          {sub && <div className="text-[8px] text-slate-600 font-mono mt-0.5">{sub}</div>}
        </div>
        {history.length > 1 && (
          <Sparkline data={history} color={sparkColor} width={80} height={24} min={sparkMin} max={sparkMax} />
        )}
      </div>
    </div>
  );
}

// ── SVG Sparkline ────────────────────────────────────────────────────

function Sparkline({
  data,
  color,
  width,
  height,
  min: forceMin,
  max: forceMax,
}: {
  data: number[];
  color: string;
  width: number;
  height: number;
  min?: number;
  max?: number;
}) {
  const min = forceMin ?? Math.min(...data);
  const max = forceMax ?? Math.max(...data);
  const range = max - min || 1;
  const pad = 1;

  const points = data.map((v, i) => {
    const x = (i / (data.length - 1)) * width;
    const y = height - pad - ((v - min) / range) * (height - pad * 2);
    return `${x},${y}`;
  });

  const linePath = `M${points.join(" L")}`;
  const areaPath = `${linePath} L${width},${height} L0,${height} Z`;

  return (
    <svg width={width} height={height} className="shrink-0">
      <defs>
        <linearGradient id={`sg-${color.replace("#", "")}`} x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={color} stopOpacity="0.3" />
          <stop offset="100%" stopColor={color} stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={areaPath} fill={`url(#sg-${color.replace("#", "")})`} />
      <path d={linePath} fill="none" stroke={color} strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  );
}

// ── Formatters ───────────────────────────────────────────────────────

function fmtRate(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  if (n >= 1) return n.toFixed(0);
  if (n > 0) return n.toFixed(1);
  return "0";
}
