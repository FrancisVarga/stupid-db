"use client";

import { useState } from "react";
import type { ConnectionInput, ConnectionSafe, TestConnectionResult } from "@/lib/api-db";
import { testConnection, addConnectionApi, updateConnectionApi } from "@/lib/api-db";

const COLORS = ["#00f0ff", "#a855f7", "#06d6a0", "#ff8a00", "#ff6eb4", "#ffe600", "#2ec4b6"];

type Mode = "url" | "manual";

interface Props {
  /** If provided, form is in edit mode for this connection. */
  editing?: ConnectionSafe;
  onSaved: () => void;
  onCancel: () => void;
}

export default function ConnectionForm({ editing, onSaved, onCancel }: Props) {
  const isEdit = !!editing;

  const [mode, setMode] = useState<Mode>("manual");
  const [connectionString, setConnectionString] = useState("");
  const [form, setForm] = useState<ConnectionInput>({
    name: editing?.name ?? "",
    host: editing?.host ?? "localhost",
    port: editing?.port ?? 5432,
    database: editing?.database ?? "",
    username: editing?.username ?? "postgres",
    password: "",
    ssl: editing?.ssl ?? false,
    color: editing?.color ?? COLORS[Math.floor(Math.random() * COLORS.length)],
  });
  const [testing, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<TestConnectionResult | null>(null);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = (key: keyof ConnectionInput, value: unknown) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  /** Build the payload — include connection_string when in URL mode. */
  const buildPayload = (): ConnectionInput | Partial<ConnectionInput> => {
    if (mode === "url" && connectionString.trim()) {
      return {
        ...form,
        connection_string: connectionString.trim(),
      };
    }
    const payload: Partial<ConnectionInput> = { ...form };
    // In edit mode, omit password if user didn't change it (empty means keep existing)
    if (isEdit && !payload.password) {
      delete payload.password;
    }
    return payload;
  };

  /** Try to auto-fill name from connection string (use database name). */
  const inferNameFromUrl = (url: string) => {
    try {
      const u = new URL(url);
      const db = u.pathname.replace(/^\//, "");
      const host = u.hostname;
      if (db && !form.name) {
        set("name", `${db}@${host}`);
      }
    } catch {
      // ignore parse errors while typing
    }
  };

  const handleTest = async () => {
    setTesting(true);
    setTestResult(null);
    setError(null);
    try {
      const payload = buildPayload();
      // For testing, parse the URL client-side to pass individual fields to the test endpoint
      if (mode === "url" && connectionString.trim()) {
        try {
          const u = new URL(connectionString.trim());
          const testPayload = {
            host: u.hostname || "localhost",
            port: parseInt(u.port) || 5432,
            database: u.pathname.replace(/^\//, "") || "postgres",
            username: decodeURIComponent(u.username) || "postgres",
            password: decodeURIComponent(u.password) || "",
            ssl: u.searchParams.get("sslmode") === "require" ||
                 u.searchParams.get("sslmode") === "verify-ca" ||
                 u.searchParams.get("sslmode") === "verify-full",
          };
          const result = await testConnection(testPayload as ConnectionInput);
          setTestResult(result);
          return;
        } catch {
          // fall through to normal test
        }
      }
      const result = await testConnection(payload as ConnectionInput);
      setTestResult(result);
    } catch (e) {
      setTestResult({ ok: false, error: (e as Error).message });
    } finally {
      setTesting(false);
    }
  };

  const handleSave = async () => {
    const payload = buildPayload();
    if (!payload.name?.trim()) {
      setError("Connection name is required.");
      return;
    }
    if (mode === "manual" && (!payload.host?.trim() || (!isEdit && !payload.database?.trim()))) {
      setError("Host and database are required.");
      return;
    }
    if (mode === "url" && !connectionString.trim()) {
      setError("Connection string is required.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      if (isEdit) {
        await updateConnectionApi(editing.id, payload);
      } else {
        await addConnectionApi(payload as ConnectionInput);
      }
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const inputClass =
    "w-full px-3 py-2 rounded-lg text-xs font-mono bg-[#0a0e15] text-slate-200 border border-slate-700/50 focus:border-cyan-500/50 focus:outline-none";

  return (
    <div
      className="rounded-xl p-6 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${isEdit ? "rgba(168, 85, 247, 0.2)" : "rgba(0, 240, 255, 0.15)"}`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${isEdit ? "rgba(168, 85, 247, 0.4)" : "rgba(0, 240, 255, 0.4)"}, transparent)`,
        }}
      />

      <div className="flex items-center justify-between mb-4">
        <h3 className="text-sm font-bold" style={{ color: isEdit ? "#a855f7" : "#00f0ff" }}>
          {isEdit ? "Edit Connection" : "Add Database Connection"}
        </h3>

        {/* Mode toggle — only show for new connections or URL mode */}
        {!isEdit && (
          <div className="flex rounded-lg overflow-hidden" style={{ border: "1px solid rgba(0, 240, 255, 0.15)" }}>
            <button
              onClick={() => setMode("url")}
              className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider transition-all"
              style={{
                background: mode === "url" ? "rgba(0, 240, 255, 0.12)" : "transparent",
                color: mode === "url" ? "#00f0ff" : "#475569",
              }}
            >
              Connection String
            </button>
            <button
              onClick={() => setMode("manual")}
              className="px-3 py-1 text-[10px] font-bold uppercase tracking-wider transition-all"
              style={{
                background: mode === "manual" ? "rgba(0, 240, 255, 0.12)" : "transparent",
                color: mode === "manual" ? "#00f0ff" : "#475569",
              }}
            >
              Manual
            </button>
          </div>
        )}
      </div>

      <div className="grid grid-cols-2 gap-3">
        {/* Name — always visible */}
        <div className="col-span-2">
          <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
            Connection Name
          </label>
          <input
            type="text"
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
            placeholder="e.g. Production DB"
            className={inputClass}
          />
        </div>

        {/* URL mode: single connection string field */}
        {mode === "url" && !isEdit && (
          <div className="col-span-2">
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Connection String
            </label>
            <input
              type="text"
              value={connectionString}
              onChange={(e) => {
                setConnectionString(e.target.value);
                inferNameFromUrl(e.target.value);
              }}
              placeholder="postgresql://user:password@host:5432/database?sslmode=require"
              className={inputClass}
              spellCheck={false}
            />
            <div className="text-[9px] text-slate-600 font-mono mt-1">
              postgresql://user:pass@host:port/dbname?sslmode=require
            </div>
          </div>
        )}

        {/* Manual mode (or edit mode): individual fields */}
        {(mode === "manual" || isEdit) && (
          <>
            {/* Host */}
            <div>
              <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
                Host
              </label>
              <input
                type="text"
                value={form.host}
                onChange={(e) => set("host", e.target.value)}
                placeholder="localhost"
                className={inputClass}
              />
            </div>

            {/* Port */}
            <div>
              <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
                Port
              </label>
              <input
                type="number"
                value={form.port}
                onChange={(e) => set("port", parseInt(e.target.value) || 5432)}
                className={inputClass}
              />
            </div>

            {/* Database */}
            <div>
              <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
                Database
              </label>
              <input
                type="text"
                value={form.database}
                onChange={(e) => set("database", e.target.value)}
                placeholder="postgres"
                className={inputClass}
              />
            </div>

            {/* Username */}
            <div>
              <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
                Username
              </label>
              <input
                type="text"
                value={form.username}
                onChange={(e) => set("username", e.target.value)}
                placeholder="postgres"
                className={inputClass}
              />
            </div>

            {/* Password */}
            <div>
              <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
                Password {isEdit && <span className="text-slate-600 normal-case">(leave blank to keep current)</span>}
              </label>
              <input
                type="password"
                value={form.password}
                onChange={(e) => set("password", e.target.value)}
                placeholder={isEdit ? "••••••••" : "••••••"}
                className={inputClass}
              />
            </div>

            {/* SSL */}
            <div className="flex items-center gap-2">
              <button
                onClick={() => set("ssl", !form.ssl)}
                className="w-8 h-4 rounded-full transition-colors relative"
                style={{
                  background: form.ssl ? "#06d6a0" : "#1e293b",
                }}
              >
                <div
                  className="w-3 h-3 rounded-full bg-white absolute top-0.5 transition-all"
                  style={{ left: form.ssl ? "17px" : "2px" }}
                />
              </button>
              <span className="text-[10px] text-slate-400">SSL</span>
            </div>
          </>
        )}

        {/* Color — always visible */}
        <div className="flex items-center gap-2 col-span-2">
          <span className="text-[10px] text-slate-500 uppercase tracking-wider">Color</span>
          <div className="flex gap-1">
            {COLORS.map((c) => (
              <button
                key={c}
                onClick={() => set("color", c)}
                className="w-4 h-4 rounded-full transition-all"
                style={{
                  background: c,
                  border: form.color === c ? "2px solid white" : "2px solid transparent",
                  transform: form.color === c ? "scale(1.2)" : "scale(1)",
                }}
              />
            ))}
          </div>
        </div>
      </div>

      {/* Test result */}
      {testResult && (
        <div
          className="mt-3 px-3 py-2 rounded-lg text-[10px] font-mono"
          style={{
            background: testResult.ok ? "rgba(6, 214, 160, 0.06)" : "rgba(255, 71, 87, 0.06)",
            border: `1px solid ${testResult.ok ? "rgba(6, 214, 160, 0.2)" : "rgba(255, 71, 87, 0.2)"}`,
            color: testResult.ok ? "#06d6a0" : "#ff4757",
          }}
        >
          {testResult.ok
            ? `Connected in ${testResult.duration_ms}ms — ${testResult.version?.split(" ").slice(0, 2).join(" ")}`
            : `Failed: ${testResult.error}`}
        </div>
      )}

      {/* Error */}
      {error && (
        <div
          className="mt-3 px-3 py-2 rounded-lg text-[10px] font-mono"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.2)",
            color: "#ff4757",
          }}
        >
          {error}
        </div>
      )}

      {/* Actions */}
      <div className="flex items-center gap-2 mt-4">
        <button
          onClick={handleTest}
          disabled={testing}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-50"
          style={{
            background: "rgba(168, 85, 247, 0.1)",
            border: "1px solid rgba(168, 85, 247, 0.3)",
            color: "#a855f7",
          }}
        >
          {testing ? "Testing..." : "Test Connection"}
        </button>
        <button
          onClick={handleSave}
          disabled={saving}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-50"
          style={{
            background: isEdit ? "rgba(168, 85, 247, 0.1)" : "rgba(0, 240, 255, 0.1)",
            border: `1px solid ${isEdit ? "rgba(168, 85, 247, 0.3)" : "rgba(0, 240, 255, 0.3)"}`,
            color: isEdit ? "#a855f7" : "#00f0ff",
          }}
        >
          {saving ? "Saving..." : isEdit ? "Update Connection" : "Save Connection"}
        </button>
        <button
          onClick={onCancel}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider text-slate-500 hover:text-slate-300 transition-colors"
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
