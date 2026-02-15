"use client";

import { useState } from "react";
import type { AthenaConnectionInput, AthenaConnectionSafe } from "@/lib/db/athena-connections";
import { addAthenaConnection, updateAthenaConnection } from "@/lib/db/athena-connections";

const COLORS = ["#10b981", "#00f0ff", "#a855f7", "#06d6a0", "#ff6eb4", "#ffe600", "#2ec4b6"];

interface Props {
  editing?: AthenaConnectionSafe;
  onSaved: () => void;
  onCancel: () => void;
}

export default function AthenaConnectionForm({ editing, onSaved, onCancel }: Props) {
  const isEdit = !!editing;

  const [form, setForm] = useState<AthenaConnectionInput>({
    name: editing?.name ?? "",
    region: editing?.region ?? "ap-southeast-1",
    catalog: editing?.catalog ?? "AwsDataCatalog",
    database: editing?.database ?? "",
    workgroup: editing?.workgroup ?? "primary",
    output_location: editing?.output_location ?? "",
    access_key_id: "",
    secret_access_key: "",
    session_token: "",
    endpoint_url: editing?.endpoint_url ?? "",
    enabled: editing?.enabled ?? true,
    color: editing?.color ?? COLORS[Math.floor(Math.random() * COLORS.length)],
  });
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const set = (key: keyof AthenaConnectionInput, value: unknown) =>
    setForm((prev) => ({ ...prev, [key]: value }));

  const buildPayload = (): Partial<AthenaConnectionInput> => {
    const payload: Partial<AthenaConnectionInput> = { ...form };
    if (isEdit) {
      if (!payload.access_key_id) delete payload.access_key_id;
      if (!payload.secret_access_key) delete payload.secret_access_key;
      if (!payload.session_token) delete payload.session_token;
    }
    return payload;
  };

  const handleSave = async () => {
    const payload = buildPayload();
    if (!payload.name?.trim()) {
      setError("Connection name is required.");
      return;
    }
    if (!payload.output_location?.trim()) {
      setError("Output location (S3 path) is required.");
      return;
    }
    setSaving(true);
    setError(null);
    try {
      if (isEdit) {
        await updateAthenaConnection(editing.id, payload);
      } else {
        await addAthenaConnection(payload as AthenaConnectionInput);
      }
      onSaved();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setSaving(false);
    }
  };

  const inputClass =
    "w-full px-3 py-2 rounded-lg text-xs font-mono bg-[#0a0e15] text-slate-200 border border-slate-700/50 focus:border-emerald-500/50 focus:outline-none";

  return (
    <div
      className="rounded-xl p-6 relative overflow-hidden"
      style={{
        background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
        border: `1px solid ${isEdit ? "rgba(168, 85, 247, 0.2)" : "rgba(16, 185, 129, 0.15)"}`,
      }}
    >
      <div
        className="absolute top-0 left-0 w-full h-[1px]"
        style={{
          background: `linear-gradient(90deg, transparent, ${isEdit ? "rgba(168, 85, 247, 0.4)" : "rgba(16, 185, 129, 0.4)"}, transparent)`,
        }}
      />

      <h3
        className="text-sm font-bold mb-4"
        style={{ color: isEdit ? "#a855f7" : "#10b981" }}
      >
        {isEdit ? "Edit Athena Connection" : "Add Athena Connection"}
      </h3>

      {/* Section 1: Connection */}
      <div className="mb-4">
        <SectionLabel text="Connection" />
        <div className="grid grid-cols-2 gap-3">
          <div className="col-span-2">
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Connection Name
            </label>
            <input
              type="text"
              value={form.name}
              onChange={(e) => set("name", e.target.value)}
              placeholder="e.g. Production Data Lake"
              className={inputClass}
            />
          </div>
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Region
            </label>
            <input
              type="text"
              value={form.region}
              onChange={(e) => set("region", e.target.value)}
              placeholder="ap-southeast-1"
              className={inputClass}
            />
          </div>
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Catalog
            </label>
            <input
              type="text"
              value={form.catalog}
              onChange={(e) => set("catalog", e.target.value)}
              placeholder="AwsDataCatalog"
              className={inputClass}
            />
          </div>
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Database
            </label>
            <input
              type="text"
              value={form.database}
              onChange={(e) => set("database", e.target.value)}
              placeholder="default"
              className={inputClass}
            />
          </div>
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Workgroup
            </label>
            <input
              type="text"
              value={form.workgroup}
              onChange={(e) => set("workgroup", e.target.value)}
              placeholder="primary"
              className={inputClass}
            />
          </div>
          <div className="col-span-2">
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Output Location
            </label>
            <input
              type="text"
              value={form.output_location}
              onChange={(e) => set("output_location", e.target.value)}
              placeholder="s3://my-bucket/athena-results/"
              className={inputClass}
              spellCheck={false}
            />
          </div>
        </div>
      </div>

      {/* Section 2: AWS Credentials */}
      <div className="mb-4">
        <SectionLabel text="AWS Credentials" />
        <div className="grid grid-cols-2 gap-3">
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Access Key ID{" "}
              {isEdit && (
                <span className="text-slate-600 normal-case">(blank = keep)</span>
              )}
            </label>
            <input
              type="password"
              value={form.access_key_id}
              onChange={(e) => set("access_key_id", e.target.value)}
              placeholder={isEdit ? "••••••••" : "AKIA..."}
              className={inputClass}
            />
          </div>
          <div>
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Secret Access Key{" "}
              {isEdit && (
                <span className="text-slate-600 normal-case">(blank = keep)</span>
              )}
            </label>
            <input
              type="password"
              value={form.secret_access_key}
              onChange={(e) => set("secret_access_key", e.target.value)}
              placeholder={isEdit ? "••••••••" : "secret"}
              className={inputClass}
            />
          </div>
          <div className="col-span-2">
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Session Token{" "}
              <span className="text-slate-600 normal-case">(optional{isEdit ? ", blank = keep" : ""})</span>
            </label>
            <input
              type="password"
              value={form.session_token}
              onChange={(e) => set("session_token", e.target.value)}
              placeholder={isEdit ? "••••••••" : "optional"}
              className={inputClass}
            />
          </div>
          <div className="col-span-2">
            <label className="text-[10px] text-slate-500 uppercase tracking-wider block mb-1">
              Endpoint URL <span className="text-slate-600 normal-case">(optional, for LocalStack/custom)</span>
            </label>
            <input
              type="text"
              value={form.endpoint_url ?? ""}
              onChange={(e) => set("endpoint_url", e.target.value || undefined)}
              placeholder="http://localhost:4566"
              className={inputClass}
              spellCheck={false}
            />
          </div>
        </div>
      </div>

      {/* Section 3: Display */}
      <div className="flex items-center gap-4 mb-4">
        <div className="flex items-center gap-2">
          <button
            onClick={() => set("enabled", !form.enabled)}
            className="w-8 h-4 rounded-full transition-colors relative"
            style={{ background: form.enabled ? "#06d6a0" : "#1e293b" }}
          >
            <div
              className="w-3 h-3 rounded-full bg-white absolute top-0.5 transition-all"
              style={{ left: form.enabled ? "17px" : "2px" }}
            />
          </button>
          <span className="text-[10px] text-slate-400">Enabled</span>
        </div>

        <div className="flex items-center gap-2">
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

      {/* Error */}
      {error && (
        <div
          className="mb-3 px-3 py-2 rounded-lg text-[10px] font-mono"
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
      <div className="flex items-center gap-2">
        <button
          onClick={handleSave}
          disabled={saving}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80 disabled:opacity-50"
          style={{
            background: isEdit ? "rgba(168, 85, 247, 0.1)" : "rgba(16, 185, 129, 0.1)",
            border: `1px solid ${isEdit ? "rgba(168, 85, 247, 0.3)" : "rgba(16, 185, 129, 0.3)"}`,
            color: isEdit ? "#a855f7" : "#10b981",
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

function SectionLabel({ text }: { text: string }) {
  return (
    <div className="text-[9px] text-slate-600 uppercase tracking-[0.15em] font-bold mb-2">
      {text}
    </div>
  );
}
