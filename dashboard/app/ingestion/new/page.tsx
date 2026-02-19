"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import Link from "next/link";
import { createIngestionSource, type CreateIngestionSource } from "@/lib/api";

// ── Types ────────────────────────────────────────────────────

type SourceType = CreateIngestionSource["source_type"];
type ZmqGranularity = NonNullable<CreateIngestionSource["zmq_granularity"]>;

interface SourceTypeOption {
  value: SourceType;
  label: string;
  description: string;
  icon: string;
}

const SOURCE_TYPES: SourceTypeOption[] = [
  { value: "parquet", label: "Parquet", description: "Upload Parquet files for one-time batch ingestion", icon: "P" },
  { value: "csv_json", label: "CSV / JSON", description: "Upload CSV or JSON files for one-time batch ingestion", icon: "C" },
  { value: "directory", label: "Directory", description: "Scan a local directory and optionally watch for new files", icon: "D" },
  { value: "s3", label: "S3", description: "Import from an S3 bucket with optional watch mode", icon: "S" },
  { value: "push", label: "Push (HTTP)", description: "Receive data via HTTP POST webhook endpoint", icon: "H" },
  { value: "queue", label: "Queue", description: "Consume from Redis, SQS, or NATS message queue", icon: "Q" },
];

const STEPS = ["Source Type", "Configuration", "ZMQ Granularity", "Review & Save"];

// ── Step Indicator ───────────────────────────────────────────

function StepIndicator({ current }: { current: number }) {
  return (
    <div className="flex items-center justify-center gap-0 mb-8">
      {STEPS.map((label, i) => (
        <div key={label} className="flex items-center">
          <div className="flex flex-col items-center">
            <div
              className={`w-8 h-8 rounded-full flex items-center justify-center text-sm font-medium border-2 transition-colors ${
                i < current
                  ? "bg-teal-500 border-teal-500 text-black"
                  : i === current
                    ? "border-teal-500 text-teal-400"
                    : "border-white/20 text-white/30"
              }`}
            >
              {i < current ? "\u2713" : i + 1}
            </div>
            <span
              className={`text-xs mt-1.5 whitespace-nowrap ${
                i <= current ? "text-teal-400" : "text-white/30"
              }`}
            >
              {label}
            </span>
          </div>
          {i < STEPS.length - 1 && (
            <div
              className={`w-12 h-0.5 mx-1 mt-[-1rem] ${
                i < current ? "bg-teal-500" : "bg-white/10"
              }`}
            />
          )}
        </div>
      ))}
    </div>
  );
}

// ── Step 1: Source Type Selection ─────────────────────────────

function StepSourceType({
  selected,
  onSelect,
}: {
  selected: SourceType | null;
  onSelect: (t: SourceType) => void;
}) {
  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">Select Source Type</h2>
      <p className="text-sm text-white/50 mb-5">Choose how data will be ingested into the system.</p>
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-3">
        {SOURCE_TYPES.map((st) => (
          <button
            key={st.value}
            type="button"
            onClick={() => onSelect(st.value)}
            className={`rounded-xl border p-4 text-left cursor-pointer transition-colors ${
              selected === st.value
                ? "border-teal-500 bg-teal-500/10"
                : "border-white/10 hover:border-teal-500/50"
            }`}
          >
            <div className="flex items-center gap-3 mb-2">
              <span className="w-8 h-8 rounded-lg bg-white/5 flex items-center justify-center text-sm font-bold text-teal-400">
                {st.icon}
              </span>
              <span className="font-medium">{st.label}</span>
            </div>
            <p className="text-xs text-white/50 leading-relaxed">{st.description}</p>
          </button>
        ))}
      </div>
    </div>
  );
}

// ── Step 2: Configuration ────────────────────────────────────

function StepConfiguration({
  sourceType,
  name,
  setName,
  config,
  setConfig,
}: {
  sourceType: SourceType;
  name: string;
  setName: (v: string) => void;
  config: Record<string, unknown>;
  setConfig: (c: Record<string, unknown>) => void;
}) {
  const set = (key: string, val: unknown) => setConfig({ ...config, [key]: val });

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">Configure Source</h2>
      <p className="text-sm text-white/50 mb-5">
        Set up the connection details for your{" "}
        <span className="text-teal-400">{SOURCE_TYPES.find((s) => s.value === sourceType)?.label}</span>{" "}
        source.
      </p>

      <div className="space-y-4">
        {/* Name — always shown */}
        <Field label="Name" required>
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="my-data-source"
            className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
          />
        </Field>

        {/* Directory fields */}
        {sourceType === "directory" && (
          <>
            <Field label="Path" required>
              <input
                value={(config.path as string) ?? ""}
                onChange={(e) => set("path", e.target.value)}
                placeholder="/data/imports"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Checkbox
              label="Watch mode"
              checked={!!config.watch}
              onChange={(v) => set("watch", v)}
            />
            {config.watch && (
              <Field label="Watch interval (seconds)">
                <input
                  type="number"
                  value={(config.watch_interval as number) ?? 30}
                  onChange={(e) => set("watch_interval", Number(e.target.value))}
                  className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
                />
              </Field>
            )}
          </>
        )}

        {/* S3 fields */}
        {sourceType === "s3" && (
          <>
            <Field label="Bucket" required>
              <input
                value={(config.bucket as string) ?? ""}
                onChange={(e) => set("bucket", e.target.value)}
                placeholder="my-bucket"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Field label="Prefix">
              <input
                value={(config.prefix as string) ?? ""}
                onChange={(e) => set("prefix", e.target.value)}
                placeholder="data/raw/"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Field label="Region" required>
              <input
                value={(config.region as string) ?? "us-east-1"}
                onChange={(e) => set("region", e.target.value)}
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Field label="Credentials ref">
              <input
                value={(config.credentials_ref as string) ?? ""}
                onChange={(e) => set("credentials_ref", e.target.value)}
                placeholder="optional"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Checkbox
              label="Watch mode"
              checked={!!config.watch}
              onChange={(v) => set("watch", v)}
            />
          </>
        )}

        {/* Push fields */}
        {sourceType === "push" && (
          <Field label="Auth token">
            <input
              value={(config.auth_token as string) ?? ""}
              onChange={(e) => set("auth_token", e.target.value)}
              placeholder="optional"
              className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
            />
          </Field>
        )}

        {/* Queue fields */}
        {sourceType === "queue" && (
          <>
            <Field label="Queue type" required>
              <select
                value={(config.queue_type as string) ?? "redis"}
                onChange={(e) => set("queue_type", e.target.value)}
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              >
                <option value="redis">Redis</option>
                <option value="sqs">SQS</option>
                <option value="nats">NATS</option>
              </select>
            </Field>
            <Field label="Connection string" required>
              <input
                value={(config.connection_string as string) ?? ""}
                onChange={(e) => set("connection_string", e.target.value)}
                placeholder="redis://localhost:6379"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Field label="Topic / Queue name" required>
              <input
                value={(config.topic as string) ?? ""}
                onChange={(e) => set("topic", e.target.value)}
                placeholder="ingestion-events"
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
            <Field label="Batch size">
              <input
                type="number"
                value={(config.batch_size as number) ?? 10}
                onChange={(e) => set("batch_size", Number(e.target.value))}
                className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
              />
            </Field>
          </>
        )}

        {/* Parquet / CSV/JSON — only name needed, show helper */}
        {(sourceType === "parquet" || sourceType === "csv_json") && (
          <p className="text-xs text-white/40 italic">
            Files will be uploaded separately after creating the source.
          </p>
        )}
      </div>
    </div>
  );
}

// ── Step 3: ZMQ Granularity ──────────────────────────────────

function StepZmqGranularity({
  selected,
  onSelect,
}: {
  selected: ZmqGranularity;
  onSelect: (v: ZmqGranularity) => void;
}) {
  const options: { value: ZmqGranularity; label: string; description: string }[] = [
    {
      value: "summary",
      label: "Summary",
      description: "Publish start and complete events only. Lower overhead.",
    },
    {
      value: "batched",
      label: "Batched",
      description: "Publish progress events every second during ingestion. Better visibility for long jobs.",
    },
  ];

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">ZMQ Event Granularity</h2>
      <p className="text-sm text-white/50 mb-5">
        Choose how much detail to publish over the event bus during ingestion.
      </p>
      <div className="space-y-3">
        {options.map((o) => (
          <button
            key={o.value}
            type="button"
            onClick={() => onSelect(o.value)}
            className={`w-full rounded-xl border p-4 text-left cursor-pointer transition-colors ${
              selected === o.value
                ? "border-teal-500 bg-teal-500/10"
                : "border-white/10 hover:border-teal-500/50"
            }`}
          >
            <span className="font-medium">{o.label}</span>
            <p className="text-xs text-white/50 mt-1">{o.description}</p>
          </button>
        ))}
      </div>

      {/* Schedule sub-section (only for applicable source types) */}
      <ScheduleSection />
    </div>
  );
}

// ── Schedule (embedded in Step 3) ────────────────────────────

function ScheduleSection() {
  // We pull schedule state from the parent via props in the real component,
  // but here we'll lift it up. See the main wizard component below.
  return null; // Placeholder — real schedule UI is in the wizard below
}

// ── Step 4: Review & Save ────────────────────────────────────

function StepReview({
  sourceType,
  name,
  config,
  zmqGranularity,
  schedule,
  submitting,
  error,
  onSubmit,
}: {
  sourceType: SourceType;
  name: string;
  config: Record<string, unknown>;
  zmqGranularity: ZmqGranularity;
  schedule: { enabled: boolean; cron: string; timezone: string };
  submitting: boolean;
  error: string | null;
  onSubmit: () => void;
}) {
  const typeLabel = SOURCE_TYPES.find((s) => s.value === sourceType)?.label ?? sourceType;
  const isFileType = sourceType === "parquet" || sourceType === "csv_json";

  return (
    <div>
      <h2 className="text-lg font-semibold mb-1">Review & Save</h2>
      <p className="text-sm text-white/50 mb-5">Verify your configuration before creating the source.</p>

      <div className="space-y-3">
        <ReviewRow label="Name" value={name} />
        <ReviewRow label="Source type" value={typeLabel} />
        <ReviewRow label="ZMQ granularity" value={zmqGranularity} />

        {/* Type-specific config */}
        {Object.entries(config).map(([k, v]) => {
          if (v === "" || v === undefined || v === null) return null;
          return <ReviewRow key={k} label={k.replace(/_/g, " ")} value={String(v)} />;
        })}

        {/* Schedule */}
        {!isFileType && schedule.enabled && (
          <>
            <ReviewRow label="Cron schedule" value={schedule.cron} />
            <ReviewRow label="Timezone" value={schedule.timezone} />
          </>
        )}
        {!isFileType && !schedule.enabled && (
          <ReviewRow label="Schedule" value="None" />
        )}
      </div>

      {error && (
        <div className="mt-4 p-3 rounded-lg bg-red-500/10 border border-red-500/30 text-red-400 text-sm">
          {error}
        </div>
      )}

      <button
        type="button"
        onClick={onSubmit}
        disabled={submitting}
        className="mt-6 w-full bg-teal-500 hover:bg-teal-400 disabled:opacity-50 text-black font-medium rounded-lg px-5 py-2.5 transition-colors"
      >
        {submitting ? "Creating..." : "Create Source"}
      </button>
    </div>
  );
}

// ── Shared small components ──────────────────────────────────

function Field({ label, required, children }: { label: string; required?: boolean; children: React.ReactNode }) {
  return (
    <label className="block">
      <span className="text-sm text-white/70 mb-1 block">
        {label}
        {required && <span className="text-teal-400 ml-1">*</span>}
      </span>
      {children}
    </label>
  );
}

function Checkbox({ label, checked, onChange }: { label: string; checked: boolean; onChange: (v: boolean) => void }) {
  return (
    <label className="flex items-center gap-2 cursor-pointer text-sm text-white/70">
      <input
        type="checkbox"
        checked={checked}
        onChange={(e) => onChange(e.target.checked)}
        className="accent-teal-500"
      />
      {label}
    </label>
  );
}

function ReviewRow({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex justify-between items-center py-2 px-3 rounded-lg bg-white/[0.03] border border-white/5">
      <span className="text-sm text-white/50 capitalize">{label}</span>
      <span className="text-sm font-medium">{value}</span>
    </div>
  );
}

// ── Main Wizard ──────────────────────────────────────────────

export default function NewIngestionSourcePage() {
  const router = useRouter();
  const [step, setStep] = useState(0);

  // Step 1
  const [sourceType, setSourceType] = useState<SourceType | null>(null);

  // Step 2
  const [name, setName] = useState("");
  const [config, setConfig] = useState<Record<string, unknown>>({});

  // Step 3
  const [zmqGranularity, setZmqGranularity] = useState<ZmqGranularity>("summary");
  const [schedule, setSchedule] = useState({ enabled: false, cron: "0 */6 * * *", timezone: "UTC" });

  // Step 4
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Reset config when source type changes
  const handleSourceTypeSelect = (t: SourceType) => {
    if (t !== sourceType) {
      setConfig(t === "s3" ? { region: "us-east-1" } : t === "queue" ? { queue_type: "redis", batch_size: 10 } : t === "directory" ? { watch_interval: 30 } : {});
    }
    setSourceType(t);
  };

  const isFileType = sourceType === "parquet" || sourceType === "csv_json";

  // Validation for "Next" button
  const canAdvance = (): boolean => {
    switch (step) {
      case 0:
        return sourceType !== null;
      case 1: {
        if (!name.trim()) return false;
        if (sourceType === "directory" && !config.path) return false;
        if (sourceType === "s3" && (!config.bucket || !config.region)) return false;
        if (sourceType === "queue" && (!config.connection_string || !config.topic)) return false;
        return true;
      }
      case 2:
        return true;
      default:
        return true;
    }
  };

  const handleSubmit = async () => {
    if (!sourceType) return;
    setSubmitting(true);
    setError(null);

    const payload: CreateIngestionSource = {
      name: name.trim(),
      source_type: sourceType,
      config_json: config,
      zmq_granularity: zmqGranularity,
      enabled: true,
    };

    if (!isFileType && schedule.enabled) {
      payload.schedule_json = { cron: schedule.cron, timezone: schedule.timezone };
    }

    try {
      await createIngestionSource(payload);
      router.push("/ingestion");
    } catch (err: unknown) {
      setError(err instanceof Error ? err.message : "Failed to create source");
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="min-h-screen bg-[#06080d] text-[#e0e6f0] p-6 max-w-2xl mx-auto">
      {/* Header */}
      <div className="flex items-center justify-between mb-6">
        <h1 className="text-xl font-bold">New Ingestion Source</h1>
        <Link
          href="/ingestion"
          className="text-sm text-white/40 hover:text-white/70 transition-colors"
        >
          Cancel
        </Link>
      </div>

      <StepIndicator current={step} />

      {/* Step content */}
      <div className="mb-8">
        {step === 0 && (
          <StepSourceType selected={sourceType} onSelect={handleSourceTypeSelect} />
        )}

        {step === 1 && sourceType && (
          <StepConfiguration
            sourceType={sourceType}
            name={name}
            setName={setName}
            config={config}
            setConfig={setConfig}
          />
        )}

        {step === 2 && (
          <div>
            <StepZmqGranularity selected={zmqGranularity} onSelect={setZmqGranularity} />

            {/* Schedule — embedded in step 3 */}
            {!isFileType ? (
              <div className="mt-6 pt-6 border-t border-white/10">
                <h3 className="text-sm font-semibold mb-3">Schedule (Optional)</h3>
                <Checkbox
                  label="Enable cron schedule"
                  checked={schedule.enabled}
                  onChange={(v) => setSchedule({ ...schedule, enabled: v })}
                />
                {schedule.enabled && (
                  <div className="mt-3 space-y-3">
                    <Field label="Cron expression">
                      <input
                        value={schedule.cron}
                        onChange={(e) => setSchedule({ ...schedule, cron: e.target.value })}
                        placeholder="0 */6 * * *"
                        className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
                      />
                    </Field>
                    <Field label="Timezone">
                      <input
                        value={schedule.timezone}
                        onChange={(e) => setSchedule({ ...schedule, timezone: e.target.value })}
                        placeholder="UTC"
                        className="bg-white/5 border border-white/10 rounded-lg px-3 py-2 w-full text-sm focus:outline-none focus:border-teal-500"
                      />
                    </Field>
                    <p className="text-xs text-white/30">
                      5-field cron: min hr dom mon dow (e.g. &ldquo;0 */6 * * *&rdquo; = every 6 hours)
                    </p>
                  </div>
                )}
              </div>
            ) : (
              <div className="mt-6 pt-6 border-t border-white/10">
                <p className="text-xs text-white/30 italic">
                  Scheduling is not available for file-based sources (Parquet / CSV/JSON).
                </p>
              </div>
            )}
          </div>
        )}

        {step === 3 && sourceType && (
          <StepReview
            sourceType={sourceType}
            name={name}
            config={config}
            zmqGranularity={zmqGranularity}
            schedule={schedule}
            submitting={submitting}
            error={error}
            onSubmit={handleSubmit}
          />
        )}
      </div>

      {/* Navigation buttons */}
      <div className="flex justify-between">
        <button
          type="button"
          onClick={() => setStep((s) => Math.max(0, s - 1))}
          className={`border border-white/20 hover:bg-white/5 rounded-lg px-5 py-2.5 text-sm transition-colors ${
            step === 0 ? "invisible" : ""
          }`}
        >
          Back
        </button>

        {step < 3 && (
          <button
            type="button"
            onClick={() => setStep((s) => s + 1)}
            disabled={!canAdvance()}
            className="bg-teal-500 hover:bg-teal-400 disabled:opacity-30 disabled:cursor-not-allowed text-black font-medium rounded-lg px-5 py-2.5 text-sm transition-colors"
          >
            Next
          </button>
        )}
      </div>
    </div>
  );
}
