"use client";

import { useState, useCallback, useEffect } from "react";
import type { Column } from "@/lib/api-db";
import JsonEditor from "./JsonEditor";

interface RecordFormProps {
  columns: Column[];
  initialData?: Record<string, unknown>;
  onSubmit: (data: Record<string, unknown>) => Promise<void>;
  onClose: () => void;
  mode: "create" | "edit";
}

// Columns that are auto-generated and should be skipped in create mode
function isAutoColumn(col: Column): boolean {
  if (col.is_pk && col.default_value?.includes("nextval")) return true;
  if (col.default_value === "now()" || col.default_value === "CURRENT_TIMESTAMP")
    return true;
  return false;
}

function defaultValueForType(udtName: string): unknown {
  switch (udtName) {
    case "bool":
      return false;
    case "int4":
    case "int8":
    case "float8":
    case "numeric":
      return "";
    case "jsonb":
    case "json":
      return "{}";
    case "timestamptz":
    case "timestamp":
      return new Date().toISOString().slice(0, 19);
    case "uuid":
      return "";
    default:
      if (udtName.startsWith("_")) return "[]";
      return "";
  }
}

export default function RecordForm({
  columns,
  initialData,
  onSubmit,
  onClose,
  mode,
}: RecordFormProps) {
  const [formData, setFormData] = useState<Record<string, unknown>>({});
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Initialize form data
  useEffect(() => {
    const data: Record<string, unknown> = {};
    for (const col of columns) {
      if (mode === "create" && isAutoColumn(col)) continue;
      if (initialData && initialData[col.name] !== undefined) {
        const val = initialData[col.name];
        // Serialize JSON values to string for the editor
        if (
          (col.udt_name === "jsonb" || col.udt_name === "json") &&
          typeof val === "object"
        ) {
          data[col.name] = JSON.stringify(val, null, 2);
        } else if (Array.isArray(val)) {
          data[col.name] = JSON.stringify(val);
        } else {
          data[col.name] = val;
        }
      } else {
        data[col.name] = defaultValueForType(col.udt_name);
      }
    }
    setFormData(data);
  }, [columns, initialData, mode]);

  const setField = useCallback((name: string, value: unknown) => {
    setFormData((prev) => ({ ...prev, [name]: value }));
  }, []);

  const handleSubmit = useCallback(
    async (e: React.FormEvent) => {
      e.preventDefault();
      setSubmitting(true);
      setError(null);
      try {
        // Convert form values to proper types before sending
        const payload: Record<string, unknown> = {};
        for (const col of columns) {
          if (mode === "create" && isAutoColumn(col)) continue;
          const raw = formData[col.name];
          if (raw === "" && col.nullable) {
            payload[col.name] = null;
            continue;
          }

          switch (col.udt_name) {
            case "jsonb":
            case "json":
              payload[col.name] =
                typeof raw === "string" ? JSON.parse(raw) : raw;
              break;
            case "bool":
              payload[col.name] = Boolean(raw);
              break;
            case "int4":
            case "int8":
              payload[col.name] = raw === "" ? null : parseInt(String(raw), 10);
              break;
            case "float8":
            case "numeric":
              payload[col.name] =
                raw === "" ? null : parseFloat(String(raw));
              break;
            default:
              if (col.udt_name.startsWith("_") && typeof raw === "string") {
                payload[col.name] = JSON.parse(raw);
              } else {
                payload[col.name] = raw;
              }
          }
        }
        await onSubmit(payload);
      } catch (err) {
        setError((err as Error).message);
      } finally {
        setSubmitting(false);
      }
    },
    [columns, formData, mode, onSubmit]
  );

  // Close on Escape
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("keydown", handler);
    return () => document.removeEventListener("keydown", handler);
  }, [onClose]);

  const editableColumns = columns.filter(
    (col) => !(mode === "create" && isAutoColumn(col))
  );

  return (
    <>
      {/* Backdrop */}
      <div
        className="fixed inset-0 z-40"
        style={{ background: "rgba(0, 0, 0, 0.6)" }}
        onClick={onClose}
      />

      {/* Modal */}
      <div
        className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-50 w-full max-w-lg max-h-[85vh] overflow-y-auto rounded-xl"
        style={{
          background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
          border: "1px solid rgba(0, 240, 255, 0.15)",
          boxShadow: "0 0 60px rgba(0, 0, 0, 0.5), 0 0 30px rgba(0, 240, 255, 0.05)",
        }}
      >
        {/* Header */}
        <div
          className="sticky top-0 z-10 px-5 py-4 flex items-center justify-between"
          style={{
            background: "rgba(12, 16, 24, 0.95)",
            backdropFilter: "blur(12px)",
            borderBottom: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          <h2
            className="text-sm font-bold tracking-wider"
            style={{ color: "#00f0ff" }}
          >
            {mode === "create" ? "New Record" : "Edit Record"}
          </h2>
          <button
            onClick={onClose}
            className="text-slate-500 hover:text-slate-300 transition-colors p-1"
          >
            <svg
              width="18"
              height="18"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
            >
              <path d="M18 6L6 18M6 6l12 12" />
            </svg>
          </button>
        </div>

        {/* Form */}
        <form onSubmit={handleSubmit} className="px-5 py-4 space-y-4">
          {editableColumns.map((col) => (
            <FieldInput
              key={col.name}
              column={col}
              value={formData[col.name]}
              onChange={(v) => setField(col.name, v)}
            />
          ))}

          {error && (
            <div
              className="rounded-lg px-3 py-2 text-xs text-red-400 font-mono"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
              }}
            >
              {error}
            </div>
          )}

          <div className="flex items-center justify-end gap-3 pt-2">
            <button
              type="button"
              onClick={onClose}
              className="px-4 py-2 text-xs font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-80"
              style={{
                color: "#64748b",
                background: "rgba(100, 116, 139, 0.08)",
                border: "1px solid rgba(100, 116, 139, 0.15)",
              }}
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={submitting}
              className="px-4 py-2 text-xs font-bold tracking-wider uppercase rounded-lg transition-all hover:opacity-90 disabled:opacity-50"
              style={{
                color: "#06080d",
                background: "#00f0ff",
              }}
            >
              {submitting
                ? "Saving..."
                : mode === "create"
                  ? "Create"
                  : "Save"}
            </button>
          </div>
        </form>
      </div>
    </>
  );
}

// ── Field Input (type-aware) ──────────────────────────────────────────

function FieldInput({
  column,
  value,
  onChange,
}: {
  column: Column;
  value: unknown;
  onChange: (v: unknown) => void;
}) {
  const label = (
    <div className="flex items-center gap-2 mb-1">
      <span className="text-[10px] text-slate-400 uppercase tracking-widest font-bold">
        {column.name}
      </span>
      <span
        className="text-[8px] font-bold uppercase tracking-wider px-1 py-0.5 rounded"
        style={{
          color: "#64748b",
          background: "rgba(100, 116, 139, 0.1)",
        }}
      >
        {column.udt_name}
      </span>
      {!column.nullable && (
        <span className="text-[8px] text-red-400 font-bold">required</span>
      )}
    </div>
  );

  const inputClass =
    "w-full bg-transparent text-xs text-slate-300 font-mono rounded-lg px-3 py-2 outline-none focus:border-cyan-800 transition-colors";
  const inputStyle = {
    background: "rgba(6, 8, 13, 0.6)",
    border: "1px solid rgba(30, 41, 59, 0.6)",
  };

  // Boolean
  if (column.udt_name === "bool") {
    return (
      <div>
        {label}
        <button
          type="button"
          onClick={() => onChange(!value)}
          className="flex items-center gap-2 px-3 py-2 rounded-lg text-xs font-mono transition-all"
          style={{
            background: value
              ? "rgba(6, 214, 160, 0.1)"
              : "rgba(255, 71, 87, 0.1)",
            border: value
              ? "1px solid rgba(6, 214, 160, 0.25)"
              : "1px solid rgba(255, 71, 87, 0.25)",
            color: value ? "#06d6a0" : "#ff4757",
          }}
        >
          <span
            className="w-3 h-3 rounded-full transition-colors"
            style={{ background: value ? "#06d6a0" : "#ff4757" }}
          />
          {value ? "true" : "false"}
        </button>
      </div>
    );
  }

  // JSON/JSONB
  if (column.udt_name === "jsonb" || column.udt_name === "json") {
    return (
      <div>
        {label}
        <JsonEditor
          value={typeof value === "string" ? value : JSON.stringify(value ?? {}, null, 2)}
          onChange={onChange}
        />
      </div>
    );
  }

  // Timestamp
  if (
    column.udt_name === "timestamptz" ||
    column.udt_name === "timestamp"
  ) {
    return (
      <div>
        {label}
        <input
          type="datetime-local"
          value={String(value ?? "").slice(0, 19)}
          onChange={(e) => onChange(e.target.value)}
          className={inputClass}
          style={inputStyle}
        />
      </div>
    );
  }

  // Numbers
  if (
    column.udt_name === "int4" ||
    column.udt_name === "int8" ||
    column.udt_name === "float8" ||
    column.udt_name === "numeric"
  ) {
    return (
      <div>
        {label}
        <input
          type="number"
          value={String(value ?? "")}
          onChange={(e) => onChange(e.target.value)}
          step={
            column.udt_name === "float8" || column.udt_name === "numeric"
              ? "any"
              : "1"
          }
          className={inputClass}
          style={inputStyle}
        />
      </div>
    );
  }

  // Arrays
  if (column.udt_name.startsWith("_")) {
    return (
      <div>
        {label}
        <JsonEditor
          value={typeof value === "string" ? value : JSON.stringify(value ?? [])}
          onChange={onChange}
        />
      </div>
    );
  }

  // Text (long)
  if (column.udt_name === "text") {
    return (
      <div>
        {label}
        <textarea
          value={String(value ?? "")}
          onChange={(e) => onChange(e.target.value)}
          rows={3}
          className={inputClass + " resize-y"}
          style={inputStyle}
        />
      </div>
    );
  }

  // Default: text input
  return (
    <div>
      {label}
      <input
        type="text"
        value={String(value ?? "")}
        onChange={(e) => onChange(e.target.value)}
        className={inputClass}
        style={inputStyle}
      />
    </div>
  );
}
