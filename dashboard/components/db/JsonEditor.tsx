"use client";

import { useState, useCallback } from "react";
import CodeEditor from "./CodeEditor";

interface JsonEditorProps {
  value: string;
  onChange: (value: string) => void;
  label?: string;
}

export default function JsonEditor({ value, onChange, label }: JsonEditorProps) {
  const [error, setError] = useState<string | null>(null);

  const handleChange = useCallback(
    (raw: string) => {
      onChange(raw);
      try {
        JSON.parse(raw);
        setError(null);
      } catch (e) {
        setError((e as Error).message);
      }
    },
    [onChange]
  );

  const handleFormat = useCallback(() => {
    try {
      const parsed = JSON.parse(value);
      const formatted = JSON.stringify(parsed, null, 2);
      onChange(formatted);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [value, onChange]);

  const handleMinify = useCallback(() => {
    try {
      const parsed = JSON.parse(value);
      const minified = JSON.stringify(parsed);
      onChange(minified);
      setError(null);
    } catch (e) {
      setError((e as Error).message);
    }
  }, [value, onChange]);

  return (
    <div className="flex flex-col gap-1.5">
      {label && (
        <label className="text-[10px] text-slate-500 uppercase tracking-widest font-bold">
          {label}
        </label>
      )}
      <div className="relative">
        <CodeEditor
          value={value}
          onChange={handleChange}
          language="json"
          minHeight="120px"
          maxHeight="300px"
        />
        <div className="absolute top-1.5 right-1.5 z-10 flex gap-1">
          <button
            type="button"
            onClick={handleFormat}
            className="text-[8px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded transition-all hover:opacity-80"
            style={{
              color: "#00f0ff",
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.15)",
            }}
          >
            Format
          </button>
          <button
            type="button"
            onClick={handleMinify}
            className="text-[8px] font-bold uppercase tracking-wider px-1.5 py-0.5 rounded transition-all hover:opacity-80"
            style={{
              color: "#64748b",
              background: "rgba(100, 116, 139, 0.08)",
              border: "1px solid rgba(100, 116, 139, 0.15)",
            }}
          >
            Minify
          </button>
        </div>
      </div>
      {error && (
        <span className="text-[10px] text-red-400 font-mono">{error}</span>
      )}
    </div>
  );
}
