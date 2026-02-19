"use client";

import { useState, useEffect, useCallback } from "react";
import { createAgent, fetchSkills, type SkillInfo } from "@/lib/api";

interface CreateAgentModalProps {
  isOpen: boolean;
  onClose: () => void;
  onCreated: () => void;
}

const PROVIDERS = ["anthropic", "openai", "gemini", "ollama"] as const;
const TIERS = ["architect", "lead", "specialist"] as const;

const MODEL_PLACEHOLDERS: Record<string, string> = {
  anthropic: "claude-sonnet-4-5-20250929",
  openai: "gpt-4o",
  gemini: "gemini-2.0-flash",
  ollama: "llama3.2",
};

const NAME_RE = /^[a-z0-9][a-z0-9-]*$/;

export default function CreateAgentModal({ isOpen, onClose, onCreated }: CreateAgentModalProps) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [tier, setTier] = useState<"architect" | "lead" | "specialist">("specialist");
  const [provider, setProvider] = useState("anthropic");
  const [model, setModel] = useState("");
  const [systemPrompt, setSystemPrompt] = useState("");
  const [temperature, setTemperature] = useState(0.7);
  const [maxTokens, setMaxTokens] = useState(4096);
  const [selectedSkills, setSelectedSkills] = useState<string[]>([]);
  const [availableSkills, setAvailableSkills] = useState<SkillInfo[]>([]);
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  // Close on Escape
  useEffect(() => {
    if (!isOpen) return;
    const handleKey = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [isOpen, onClose]);

  // Reset form and load skills when opened
  useEffect(() => {
    if (isOpen) {
      setName("");
      setDescription("");
      setTier("specialist");
      setProvider("anthropic");
      setModel("");
      setSystemPrompt("");
      setTemperature(0.7);
      setMaxTokens(4096);
      setSelectedSkills([]);
      setError(null);
      fetchSkills().then(setAvailableSkills).catch(() => setAvailableSkills([]));
    }
  }, [isOpen]);

  const nameError = name.length > 0 && !NAME_RE.test(name)
    ? "Must be lowercase alphanumeric with hyphens, cannot start with hyphen"
    : null;

  const canSubmit = name.length > 0 && !nameError && !submitting;

  const addSkill = (skillName: string) => {
    if (!selectedSkills.includes(skillName)) {
      setSelectedSkills((prev) => [...prev, skillName]);
    }
  };

  const removeSkill = (skillName: string) => {
    setSelectedSkills((prev) => prev.filter((s) => s !== skillName));
  };

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);
    try {
      await createAgent({
        name,
        description,
        tier,
        provider: {
          type: provider,
          model: model || MODEL_PLACEHOLDERS[provider] || "",
        },
        system_prompt: systemPrompt,
        tags: [],
        skills: [],
        skill_refs: selectedSkills,
      });
      onCreated();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to create agent");
    } finally {
      setSubmitting(false);
    }
  }, [canSubmit, name, description, tier, provider, model, systemPrompt, selectedSkills, onCreated, onClose]);

  if (!isOpen) return null;

  const unselectedSkills = availableSkills.filter(
    (s) => !selectedSkills.includes(s.name)
  );

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
      onClick={onClose}
    >
      <div
        className="w-full max-w-2xl rounded-2xl overflow-hidden max-h-[90vh] flex flex-col"
        style={{
          background: "#111827",
          border: "1px solid rgba(251, 191, 36, 0.2)",
          boxShadow: "0 0 60px rgba(251, 191, 36, 0.06), 0 25px 50px rgba(0, 0, 0, 0.5)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="px-6 py-4 flex items-center justify-between shrink-0"
          style={{
            borderBottom: "1px solid rgba(251, 191, 36, 0.1)",
            background: "linear-gradient(180deg, rgba(251, 191, 36, 0.03) 0%, transparent 100%)",
          }}
        >
          <div className="flex items-center gap-3">
            <span className="text-sm font-bold tracking-wider" style={{ color: "#fbbf24" }}>
              Create Agent
            </span>
            <span
              className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(251, 191, 36, 0.08)",
                color: "#fbbf24",
                border: "1px solid rgba(251, 191, 36, 0.15)",
              }}
            >
              New
            </span>
          </div>
          <button
            onClick={onClose}
            className="text-slate-500 hover:text-slate-300 transition-colors text-lg leading-none px-1"
            title="Close (Esc)"
          >
            &times;
          </button>
        </div>

        {/* Body */}
        <div className="flex-1 overflow-y-auto px-6 py-5 space-y-5">
          {/* Error */}
          {error && (
            <div
              className="px-4 py-3 rounded-lg text-xs"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
                color: "#ff4757",
              }}
            >
              {error}
            </div>
          )}

          {/* Name */}
          <Field label="Name" required>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value.toLowerCase())}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: `1px solid ${nameError ? "rgba(255, 71, 87, 0.3)" : "rgba(0, 240, 255, 0.1)"}`,
              }}
              placeholder="my-agent-name"
            />
            {nameError && (
              <p className="text-[10px] text-red-400 mt-1 font-mono">{nameError}</p>
            )}
          </Field>

          {/* Description */}
          <Field label="Description">
            <textarea
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              rows={2}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono resize-y outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(0, 240, 255, 0.1)",
              }}
              placeholder="What does this agent do?"
            />
          </Field>

          {/* Tier & Provider */}
          <div className="grid grid-cols-2 gap-4">
            <Field label="Tier">
              <select
                value={tier}
                onChange={(e) => setTier(e.target.value as typeof tier)}
                className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 outline-none cursor-pointer"
                style={{
                  background: "#0f1724",
                  border: "1px solid rgba(0, 240, 255, 0.1)",
                }}
              >
                {TIERS.map((t) => (
                  <option key={t} value={t}>{t}</option>
                ))}
              </select>
            </Field>

            <Field label="Provider">
              <select
                value={provider}
                onChange={(e) => setProvider(e.target.value)}
                className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 outline-none cursor-pointer"
                style={{
                  background: "#0f1724",
                  border: "1px solid rgba(0, 240, 255, 0.1)",
                }}
              >
                {PROVIDERS.map((p) => (
                  <option key={p} value={p}>{p}</option>
                ))}
              </select>
            </Field>
          </div>

          {/* Model */}
          <Field label="Model">
            <input
              type="text"
              value={model}
              onChange={(e) => setModel(e.target.value)}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(0, 240, 255, 0.1)",
              }}
              placeholder={MODEL_PLACEHOLDERS[provider] || "model-name"}
            />
          </Field>

          {/* System Prompt */}
          <Field label="System Prompt">
            <textarea
              value={systemPrompt}
              onChange={(e) => setSystemPrompt(e.target.value)}
              rows={6}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono resize-y outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(0, 240, 255, 0.1)",
                minHeight: "120px",
              }}
              placeholder="You are a helpful agent..."
            />
          </Field>

          {/* Skills */}
          <Field label="Skills">
            {/* Selected skills as tags */}
            {selectedSkills.length > 0 && (
              <div className="flex flex-wrap gap-1.5 mb-2">
                {selectedSkills.map((skillName) => (
                  <span
                    key={skillName}
                    className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-mono"
                    style={{
                      background: "rgba(168, 85, 247, 0.1)",
                      border: "1px solid rgba(168, 85, 247, 0.2)",
                      color: "#a855f7",
                    }}
                  >
                    {skillName}
                    <button
                      onClick={() => removeSkill(skillName)}
                      className="text-slate-500 hover:text-red-400 transition-colors ml-0.5 text-sm leading-none"
                    >
                      &times;
                    </button>
                  </span>
                ))}
              </div>
            )}

            {/* Skill selector */}
            {unselectedSkills.length > 0 ? (
              <select
                value=""
                onChange={(e) => {
                  if (e.target.value) addSkill(e.target.value);
                }}
                className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 outline-none cursor-pointer"
                style={{
                  background: "#0f1724",
                  border: "1px solid rgba(168, 85, 247, 0.15)",
                }}
              >
                <option value="">Add a skill...</option>
                {unselectedSkills.map((s) => (
                  <option key={s.name} value={s.name}>
                    {s.name}{s.description ? ` — ${s.description}` : ""}
                  </option>
                ))}
              </select>
            ) : (
              <div className="text-[10px] text-slate-600 font-mono">
                {availableSkills.length === 0
                  ? "No skills available. Create skills in the Skills tab first."
                  : "All available skills selected."}
              </div>
            )}
          </Field>

          {/* Temperature & Max Tokens */}
          <div className="grid grid-cols-2 gap-4">
            <Field label={`Temperature: ${temperature.toFixed(1)}`}>
              <input
                type="range"
                min={0}
                max={2}
                step={0.1}
                value={temperature}
                onChange={(e) => setTemperature(parseFloat(e.target.value))}
                className="w-full accent-amber-400"
              />
              <div className="flex justify-between text-[9px] font-mono text-slate-600 mt-1">
                <span>0.0</span>
                <span>1.0</span>
                <span>2.0</span>
              </div>
            </Field>

            <Field label="Max Tokens">
              <input
                type="number"
                value={maxTokens}
                onChange={(e) => setMaxTokens(parseInt(e.target.value) || 0)}
                className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
                style={{
                  background: "#0f1724",
                  border: "1px solid rgba(0, 240, 255, 0.1)",
                }}
                min={1}
              />
            </Field>
          </div>
        </div>

        {/* Footer */}
        <div
          className="px-6 py-4 flex items-center justify-end gap-3 shrink-0"
          style={{
            borderTop: "1px solid rgba(251, 191, 36, 0.1)",
            background: "linear-gradient(0deg, rgba(251, 191, 36, 0.02) 0%, transparent 100%)",
          }}
        >
          <button
            onClick={onClose}
            className="px-4 py-2 rounded-lg text-sm font-medium text-slate-500 hover:text-slate-300 transition-colors"
            style={{ border: "1px solid rgba(100, 116, 139, 0.15)" }}
          >
            Cancel
          </button>
          <button
            onClick={handleSubmit}
            disabled={!canSubmit}
            className="px-5 py-2 rounded-lg text-sm font-bold tracking-wider uppercase transition-opacity hover:opacity-80 disabled:opacity-30"
            style={{
              background: "rgba(251, 191, 36, 0.15)",
              border: "1px solid rgba(251, 191, 36, 0.3)",
              color: "#fbbf24",
            }}
          >
            {submitting ? "Creating..." : "Create Agent"}
          </button>
        </div>
      </div>
    </div>
  );
}

function Field({ label, required, children }: { label: string; required?: boolean; children: React.ReactNode }) {
  return (
    <div>
      <label className="block text-[10px] font-bold uppercase tracking-[0.15em] text-slate-500 mb-2">
        {label}
        {required && <span className="text-amber-400 ml-1">*</span>}
      </label>
      {children}
    </div>
  );
}
