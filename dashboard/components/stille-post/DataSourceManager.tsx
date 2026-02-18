"use client";

import { useState, useEffect, useCallback, useRef } from "react";

/* ─── Types ──────────────────────────────────── */

type SourceType = "athena" | "s3" | "api" | "upload";

interface DataSource {
  id: number;
  name: string;
  source_type: SourceType;
  config_json: Record<string, unknown>;
  created_at: string;
  updated_at: string;
}

interface TestResult {
  success: boolean;
  message: string;
}

const SOURCE_TYPES: { value: SourceType; label: string; color: string }[] = [
  { value: "athena", label: "Athena", color: "#00f0ff" },
  { value: "s3", label: "S3", color: "#06d6a0" },
  { value: "api", label: "API", color: "#a855f7" },
  { value: "upload", label: "Upload", color: "#f97316" },
];

/* ─── API helpers ────────────────────────────── */

const API = "/api/stille-post/data-sources";

async function fetchSources(): Promise<DataSource[]> {
  const res = await fetch(API);
  if (!res.ok) throw new Error(`Failed to load data sources (${res.status})`);
  return res.json();
}

async function createSource(body: {
  name: string;
  source_type: SourceType;
  config_json: Record<string, unknown>;
}): Promise<DataSource> {
  const res = await fetch(API, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Create failed (${res.status})`);
  return res.json();
}

async function updateSource(
  id: number,
  body: { name: string; source_type: SourceType; config_json: Record<string, unknown> },
): Promise<DataSource> {
  const res = await fetch(`${API}/${id}`, {
    method: "PUT",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`Update failed (${res.status})`);
  return res.json();
}

async function deleteSource(id: number): Promise<void> {
  const res = await fetch(`${API}/${id}`, { method: "DELETE" });
  if (!res.ok) throw new Error(`Delete failed (${res.status})`);
}

async function testConnection(id: number): Promise<TestResult> {
  const res = await fetch(`${API}/${id}/test`, { method: "POST" });
  if (!res.ok) throw new Error(`Test failed (${res.status})`);
  return res.json();
}

async function uploadFile(file: File): Promise<DataSource> {
  const form = new FormData();
  form.append("file", file);
  const res = await fetch(`${API}/upload`, { method: "POST", body: form });
  if (!res.ok) throw new Error(`Upload failed (${res.status})`);
  return res.json();
}

/* ─── Main component ─────────────────────────── */

export default function DataSourceManager() {
  const [sources, setSources] = useState<DataSource[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [showForm, setShowForm] = useState(false);
  const [editing, setEditing] = useState<DataSource | null>(null);
  const [testResults, setTestResults] = useState<Record<number, TestResult>>({});
  const [testingIds, setTestingIds] = useState<Set<number>>(new Set());
  const [deletingId, setDeletingId] = useState<number | null>(null);

  const reload = useCallback(() => {
    setError(null);
    fetchSources()
      .then(setSources)
      .catch((e) => setError((e as Error).message))
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    reload();
  }, [reload]);

  const handleTest = useCallback(async (id: number) => {
    setTestingIds((prev) => new Set(prev).add(id));
    try {
      const result = await testConnection(id);
      setTestResults((prev) => ({ ...prev, [id]: result }));
    } catch (e) {
      setTestResults((prev) => ({
        ...prev,
        [id]: { success: false, message: (e as Error).message },
      }));
    } finally {
      setTestingIds((prev) => {
        const next = new Set(prev);
        next.delete(id);
        return next;
      });
    }
  }, []);

  const handleDelete = useCallback(
    async (id: number) => {
      setDeletingId(id);
      try {
        await deleteSource(id);
        reload();
      } catch (e) {
        setError((e as Error).message);
      } finally {
        setDeletingId(null);
      }
    },
    [reload],
  );

  const handleCreated = useCallback(() => {
    setShowForm(false);
    setEditing(null);
    reload();
  }, [reload]);

  const handleEdit = useCallback((ds: DataSource) => {
    setEditing(ds);
    setShowForm(true);
  }, []);

  const handleCancel = useCallback(() => {
    setShowForm(false);
    setEditing(null);
  }, []);

  return (
    <div className="space-y-4">
      {/* Header row */}
      <div className="flex items-center justify-between">
        <h3
          className="text-[10px] font-bold uppercase tracking-[0.15em]"
          style={{ color: "#06d6a0" }}
        >
          Data Sources
        </h3>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="px-3 py-1.5 rounded-lg text-[10px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#06d6a0",
              border: "1px solid rgba(6, 214, 160, 0.3)",
              background: "rgba(6, 214, 160, 0.06)",
            }}
          >
            + New Source
          </button>
        )}
      </div>

      {/* Error banner */}
      {error && (
        <div
          className="flex items-center gap-2 px-3 py-2 rounded-lg"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span
            className="w-1.5 h-1.5 rounded-full shrink-0 animate-pulse"
            style={{ background: "#ff4757" }}
          />
          <span className="text-[10px] text-red-400 font-mono flex-1">{error}</span>
          <button
            onClick={() => setError(null)}
            className="text-[9px] text-red-400 hover:text-red-300 font-mono"
          >
            dismiss
          </button>
        </div>
      )}

      {/* Create / Edit form */}
      {showForm && (
        <DataSourceForm
          editing={editing}
          onSaved={handleCreated}
          onCancel={handleCancel}
          onReload={reload}
        />
      )}

      {/* Loading */}
      {loading && (
        <div className="py-8 text-center">
          <span className="text-[10px] text-slate-600 font-mono animate-pulse">
            Loading data sources...
          </span>
        </div>
      )}

      {/* Empty state */}
      {!loading && sources.length === 0 && !showForm && (
        <div
          className="rounded-xl px-4 py-8 text-center"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
          }}
        >
          <p className="text-sm text-slate-500 font-mono">No data sources configured</p>
          <p className="text-[10px] text-slate-600 font-mono mt-1">
            Create one to start importing data
          </p>
        </div>
      )}

      {/* Source list */}
      {!loading && sources.length > 0 && (
        <div
          className="rounded-xl overflow-hidden"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.08)",
          }}
        >
          {/* Table header */}
          <div
            className="grid gap-2 px-4 py-2 text-[9px] font-bold uppercase tracking-[0.15em] text-slate-500 font-mono"
            style={{
              gridTemplateColumns: "1fr 100px 140px 120px 160px",
              borderBottom: "1px solid rgba(0, 240, 255, 0.06)",
            }}
          >
            <span>Name</span>
            <span>Type</span>
            <span>Created</span>
            <span>Status</span>
            <span className="text-right">Actions</span>
          </div>

          {/* Rows */}
          {sources.map((ds, i) => {
            const typeInfo = SOURCE_TYPES.find((t) => t.value === ds.source_type);
            const test = testResults[ds.id];
            const isTesting = testingIds.has(ds.id);
            const isDeleting = deletingId === ds.id;

            return (
              <div
                key={ds.id}
                className="grid gap-2 px-4 py-2.5 items-center text-[11px] font-mono transition-colors"
                style={{
                  gridTemplateColumns: "1fr 100px 140px 120px 160px",
                  background:
                    i % 2 === 0 ? "rgba(15, 23, 42, 0.3)" : "rgba(15, 23, 42, 0.5)",
                }}
              >
                {/* Name */}
                <span className="text-slate-200 truncate">{ds.name}</span>

                {/* Type badge */}
                <span>
                  <span
                    className="px-1.5 py-0.5 rounded text-[8px] font-bold uppercase tracking-wider"
                    style={{
                      color: typeInfo?.color ?? "#6b7280",
                      background: `${typeInfo?.color ?? "#6b7280"}15`,
                      border: `1px solid ${typeInfo?.color ?? "#6b7280"}30`,
                    }}
                  >
                    {ds.source_type}
                  </span>
                </span>

                {/* Created date */}
                <span className="text-slate-500 text-[10px]">
                  {new Date(ds.created_at).toLocaleDateString()}
                </span>

                {/* Test status */}
                <span>
                  {isTesting && (
                    <span className="text-[9px] text-slate-400 animate-pulse">
                      Testing...
                    </span>
                  )}
                  {!isTesting && test && (
                    <span
                      className="text-[9px] font-bold"
                      style={{ color: test.success ? "#06d6a0" : "#ff4757" }}
                    >
                      {test.success ? "Connected" : "Failed"}
                    </span>
                  )}
                  {!isTesting && !test && (
                    <span className="text-[9px] text-slate-600">—</span>
                  )}
                </span>

                {/* Actions */}
                <div className="flex items-center gap-1.5 justify-end">
                  <button
                    onClick={() => handleTest(ds.id)}
                    disabled={isTesting}
                    className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
                    style={{
                      color: "#00f0ff",
                      border: "1px solid rgba(0, 240, 255, 0.2)",
                      background: "rgba(0, 240, 255, 0.04)",
                    }}
                  >
                    Test
                  </button>
                  <button
                    onClick={() => handleEdit(ds)}
                    className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
                    style={{
                      color: "#a855f7",
                      border: "1px solid rgba(168, 85, 247, 0.2)",
                      background: "rgba(168, 85, 247, 0.04)",
                    }}
                  >
                    Edit
                  </button>
                  <button
                    onClick={() => handleDelete(ds.id)}
                    disabled={isDeleting}
                    className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
                    style={{
                      color: "#ff4757",
                      border: "1px solid rgba(255, 71, 87, 0.2)",
                      background: "rgba(255, 71, 87, 0.04)",
                    }}
                  >
                    Del
                  </button>
                </div>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}

/* ─── Create / Edit Form ─────────────────────── */

interface DataSourceFormProps {
  editing: DataSource | null;
  onSaved: () => void;
  onCancel: () => void;
  onReload: () => void;
}

function DataSourceForm({ editing, onSaved, onCancel }: DataSourceFormProps) {
  const [name, setName] = useState(editing?.name ?? "");
  const [sourceType, setSourceType] = useState<SourceType>(editing?.source_type ?? "athena");
  const [saving, setSaving] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);

  // Type-specific config state
  const cfg = (editing?.config_json ?? {}) as Record<string, string>;
  const [athenaQuery, setAthenaQuery] = useState(cfg.query ?? "");
  const [s3Bucket, setS3Bucket] = useState(cfg.bucket ?? "");
  const [s3Prefix, setS3Prefix] = useState(cfg.prefix ?? "");
  const [s3Region, setS3Region] = useState(cfg.region ?? "eu-central-1");
  const [apiUrl, setApiUrl] = useState(cfg.url ?? "");
  const [apiMethod, setApiMethod] = useState(cfg.method ?? "GET");
  const [apiHeaders, setApiHeaders] = useState(cfg.headers ?? "");
  const [selectedFile, setSelectedFile] = useState<File | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const dropRef = useRef<HTMLDivElement>(null);
  const [dragging, setDragging] = useState(false);

  const buildConfig = (): Record<string, unknown> => {
    switch (sourceType) {
      case "athena":
        return { query: athenaQuery };
      case "s3":
        return { bucket: s3Bucket, prefix: s3Prefix, region: s3Region };
      case "api":
        return { url: apiUrl, method: apiMethod, headers: apiHeaders };
      case "upload":
        return {};
    }
  };

  const handleSubmit = async () => {
    if (!name.trim()) {
      setFormError("Name is required");
      return;
    }

    setSaving(true);
    setFormError(null);

    try {
      if (sourceType === "upload" && !editing && selectedFile) {
        await uploadFile(selectedFile);
      } else if (editing) {
        await updateSource(editing.id, {
          name: name.trim(),
          source_type: sourceType,
          config_json: buildConfig(),
        });
      } else {
        await createSource({
          name: name.trim(),
          source_type: sourceType,
          config_json: buildConfig(),
        });
      }
      onSaved();
    } catch (e) {
      setFormError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  // Drag and drop handlers
  const handleDragOver = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragging(true);
  }, []);

  const handleDragLeave = useCallback(() => {
    setDragging(false);
  }, []);

  const handleDrop = useCallback((e: React.DragEvent) => {
    e.preventDefault();
    setDragging(false);
    const file = e.dataTransfer.files[0];
    if (file) setSelectedFile(file);
  }, []);

  const inputStyle = {
    background: "rgba(255, 255, 255, 0.03)",
    border: "1px solid rgba(0, 240, 255, 0.08)",
    color: "#e2e8f0",
  };

  return (
    <div
      className="rounded-xl p-5 relative overflow-hidden space-y-4"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: "1px solid rgba(6, 214, 160, 0.15)",
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background:
            "linear-gradient(90deg, transparent, rgba(6, 214, 160, 0.4), transparent)",
        }}
      />

      <h4
        className="text-[10px] font-bold uppercase tracking-[0.15em]"
        style={{ color: "#06d6a0" }}
      >
        {editing ? "Edit Data Source" : "New Data Source"}
      </h4>

      {/* Name */}
      <div className="space-y-1">
        <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
          Name
        </label>
        <input
          type="text"
          value={name}
          onChange={(e) => setName(e.target.value)}
          placeholder="My data source"
          className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
          style={inputStyle}
        />
      </div>

      {/* Source type selector */}
      <div className="space-y-1">
        <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
          Source Type
        </label>
        <div className="flex gap-2">
          {SOURCE_TYPES.map((t) => (
            <button
              key={t.value}
              onClick={() => setSourceType(t.value)}
              className="px-3 py-1.5 rounded-lg text-[10px] font-bold font-mono uppercase tracking-wider transition-all"
              style={{
                color: sourceType === t.value ? t.color : "#475569",
                border: `1px solid ${sourceType === t.value ? `${t.color}40` : "rgba(71, 85, 105, 0.2)"}`,
                background:
                  sourceType === t.value
                    ? `${t.color}10`
                    : "rgba(71, 85, 105, 0.04)",
              }}
            >
              {t.label}
            </button>
          ))}
        </div>
      </div>

      {/* Dynamic config form */}
      <div className="space-y-3">
        {sourceType === "athena" && (
          <div className="space-y-1">
            <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
              Query
            </label>
            <textarea
              value={athenaQuery}
              onChange={(e) => setAthenaQuery(e.target.value)}
              placeholder="SELECT * FROM ..."
              rows={4}
              className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none resize-y"
              style={inputStyle}
            />
          </div>
        )}

        {sourceType === "s3" && (
          <div className="grid grid-cols-3 gap-3">
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Bucket
              </label>
              <input
                type="text"
                value={s3Bucket}
                onChange={(e) => setS3Bucket(e.target.value)}
                placeholder="my-bucket"
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Prefix
              </label>
              <input
                type="text"
                value={s3Prefix}
                onChange={(e) => setS3Prefix(e.target.value)}
                placeholder="data/"
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Region
              </label>
              <input
                type="text"
                value={s3Region}
                onChange={(e) => setS3Region(e.target.value)}
                placeholder="eu-central-1"
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
          </div>
        )}

        {sourceType === "api" && (
          <div className="space-y-3">
            <div className="grid grid-cols-4 gap-3">
              <div className="col-span-3 space-y-1">
                <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                  URL
                </label>
                <input
                  type="text"
                  value={apiUrl}
                  onChange={(e) => setApiUrl(e.target.value)}
                  placeholder="https://api.example.com/data"
                  className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                  style={inputStyle}
                />
              </div>
              <div className="space-y-1">
                <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                  Method
                </label>
                <select
                  value={apiMethod}
                  onChange={(e) => setApiMethod(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none cursor-pointer"
                  style={inputStyle}
                >
                  <option value="GET">GET</option>
                  <option value="POST">POST</option>
                </select>
              </div>
            </div>
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Headers (JSON)
              </label>
              <input
                type="text"
                value={apiHeaders}
                onChange={(e) => setApiHeaders(e.target.value)}
                placeholder='{"Authorization": "Bearer ..."}'
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
          </div>
        )}

        {sourceType === "upload" && (
          <div className="space-y-1">
            <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
              File
            </label>
            <div
              ref={dropRef}
              onDragOver={handleDragOver}
              onDragLeave={handleDragLeave}
              onDrop={handleDrop}
              onClick={() => fileInputRef.current?.click()}
              className="rounded-lg px-4 py-6 text-center cursor-pointer transition-all"
              style={{
                background: dragging
                  ? "rgba(6, 214, 160, 0.06)"
                  : "rgba(255, 255, 255, 0.02)",
                border: `2px dashed ${dragging ? "rgba(6, 214, 160, 0.4)" : "rgba(0, 240, 255, 0.1)"}`,
              }}
            >
              <input
                ref={fileInputRef}
                type="file"
                className="hidden"
                onChange={(e) => {
                  const f = e.target.files?.[0];
                  if (f) setSelectedFile(f);
                }}
              />
              {uploadFile ? (
                <div>
                  <span className="text-[11px] text-slate-300 font-mono">
                    {uploadFile.name}
                  </span>
                  <span className="text-[9px] text-slate-500 font-mono ml-2">
                    ({(uploadFile.size / 1024).toFixed(1)} KB)
                  </span>
                </div>
              ) : (
                <div>
                  <p className="text-[11px] text-slate-400 font-mono">
                    Drop file here or click to browse
                  </p>
                  <p className="text-[9px] text-slate-600 font-mono mt-1">
                    CSV, JSON, Parquet
                  </p>
                </div>
              )}
            </div>
          </div>
        )}
      </div>

      {/* Form error */}
      {formError && (
        <div
          className="flex items-center gap-2 px-3 py-2 rounded-lg"
          style={{
            background: "rgba(255, 71, 87, 0.06)",
            border: "1px solid rgba(255, 71, 87, 0.15)",
          }}
        >
          <span
            className="w-1.5 h-1.5 rounded-full shrink-0"
            style={{ background: "#ff4757" }}
          />
          <span className="text-[10px] text-red-400 font-mono">{formError}</span>
        </div>
      )}

      {/* Buttons */}
      <div className="flex items-center gap-2 pt-1">
        <button
          onClick={handleSubmit}
          disabled={saving}
          className="px-4 py-1.5 rounded-lg text-[10px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
          style={{
            color: "#06d6a0",
            border: "1px solid rgba(6, 214, 160, 0.3)",
            background: "rgba(6, 214, 160, 0.08)",
          }}
        >
          {saving ? "Saving..." : editing ? "Update" : "Create"}
        </button>
        <button
          onClick={onCancel}
          disabled={saving}
          className="px-4 py-1.5 rounded-lg text-[10px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-40"
          style={{
            color: "#6b7280",
            border: "1px solid rgba(107, 114, 128, 0.2)",
            background: "rgba(107, 114, 128, 0.04)",
          }}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}
