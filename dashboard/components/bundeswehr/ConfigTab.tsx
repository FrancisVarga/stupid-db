"use client";

import { useEffect, useState, useCallback } from "react";
import { fetchAgentDetail, updateAgent, type AgentDetail } from "@/lib/api";

interface ConfigTabProps {
  agentName: string;
}

const PROVIDERS = ["anthropic", "openai", "gemini", "ollama"] as const;
const TIERS = ["architect", "lead", "specialist"] as const;

export default function ConfigTab({ agentName }: ConfigTabProps) {
  const [agent, setAgent] = useState<AgentDetail | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [dirty, setDirty] = useState(false);

  // Editable fields
  const [systemPrompt, setSystemPrompt] = useState("");
  const [providerType, setProviderType] = useState("anthropic");
  const [model, setModel] = useState("");
  const [temperature, setTemperature] = useState(0.7);
  const [maxTokens, setMaxTokens] = useState(4096);
  const [tier, setTier] = useState<"architect" | "lead" | "specialist">("specialist");
  const [tags, setTags] = useState<string[]>([]);
  const [tagInput, setTagInput] = useState("");
  const [skills, setSkills] = useState<{ name: string; prompt: string }[]>([]);
  const [showSkillForm, setShowSkillForm] = useState(false);
  const [newSkillName, setNewSkillName] = useState("");
  const [newSkillPrompt, setNewSkillPrompt] = useState("");

  const loadAgent = useCallback(() => {
    setLoading(true);
    setError(null);
    fetchAgentDetail(agentName)
      .then((data) => {
        setAgent(data);
        setSystemPrompt(data.system_prompt);
        setProviderType(data.provider.type);
        setModel(data.provider.model);
        setTier(data.tier);
        setTags([...data.tags]);
        setSkills(data.skills.map((s) => ({ ...s })));
        setDirty(false);
      })
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load agent"))
      .finally(() => setLoading(false));
  }, [agentName]);

  useEffect(() => { loadAgent(); }, [loadAgent]);

  const markDirty = () => setDirty(true);

  const handleSave = async () => {
    setSaving(true);
    setError(null);
    try {
      const updated = await updateAgent(agentName, {
        system_prompt: systemPrompt,
        provider: { type: providerType, model },
        tier,
        tags,
        skills,
      });
      setAgent(updated);
      setDirty(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to save");
    } finally {
      setSaving(false);
    }
  };

  const handleReset = () => {
    loadAgent();
  };

  const addTag = () => {
    const t = tagInput.trim().toLowerCase();
    if (t && !tags.includes(t)) {
      setTags([...tags, t]);
      markDirty();
    }
    setTagInput("");
  };

  const removeTag = (tag: string) => {
    setTags(tags.filter((t) => t !== tag));
    markDirty();
  };

  const addSkill = () => {
    const name = newSkillName.trim();
    const prompt = newSkillPrompt.trim();
    if (!name || !prompt) return;
    setSkills([...skills, { name, prompt }]);
    setNewSkillName("");
    setNewSkillPrompt("");
    setShowSkillForm(false);
    markDirty();
  };

  const removeSkill = (idx: number) => {
    setSkills(skills.filter((_, i) => i !== idx));
    markDirty();
  };

  if (loading) {
    return (
      <div className="space-y-4">
        {Array.from({ length: 5 }).map((_, i) => (
          <div
            key={i}
            className="h-12 rounded-lg animate-pulse"
            style={{ background: "rgba(0, 240, 255, 0.03)" }}
          />
        ))}
      </div>
    );
  }

  if (error && !agent) {
    return (
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
    );
  }

  return (
    <div className="space-y-6">
      {/* Save error */}
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

      {/* System Prompt */}
      <FieldSection label="System Prompt">
        <textarea
          value={systemPrompt}
          onChange={(e) => { setSystemPrompt(e.target.value); markDirty(); }}
          rows={12}
          className="w-full rounded-lg px-4 py-3 text-sm text-slate-200 font-mono resize-y outline-none focus:ring-1"
          style={{
            background: "#0f1724",
            border: "1px solid rgba(0, 240, 255, 0.1)",
            minHeight: "200px",
          }}
          placeholder="Enter system prompt..."
        />
      </FieldSection>

      {/* Provider & Model */}
      <div className="grid grid-cols-2 gap-4">
        <FieldSection label="Provider">
          <select
            value={providerType}
            onChange={(e) => { setProviderType(e.target.value); markDirty(); }}
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
        </FieldSection>

        <FieldSection label="Model">
          <input
            type="text"
            value={model}
            onChange={(e) => { setModel(e.target.value); markDirty(); }}
            className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
            style={{
              background: "#0f1724",
              border: "1px solid rgba(0, 240, 255, 0.1)",
            }}
            placeholder="e.g. claude-sonnet-4-5-20250929"
          />
        </FieldSection>
      </div>

      {/* Execution Settings */}
      <div className="grid grid-cols-2 gap-4">
        <FieldSection label={`Temperature: ${temperature.toFixed(1)}`}>
          <input
            type="range"
            min={0}
            max={2}
            step={0.1}
            value={temperature}
            onChange={(e) => { setTemperature(parseFloat(e.target.value)); markDirty(); }}
            className="w-full accent-amber-400"
          />
          <div className="flex justify-between text-[9px] font-mono text-slate-600 mt-1">
            <span>0.0</span>
            <span>1.0</span>
            <span>2.0</span>
          </div>
        </FieldSection>

        <FieldSection label="Max Tokens">
          <input
            type="number"
            value={maxTokens}
            onChange={(e) => { setMaxTokens(parseInt(e.target.value) || 0); markDirty(); }}
            className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
            style={{
              background: "#0f1724",
              border: "1px solid rgba(0, 240, 255, 0.1)",
            }}
            min={1}
          />
        </FieldSection>
      </div>

      {/* Tier */}
      <FieldSection label="Tier">
        <select
          value={tier}
          onChange={(e) => { setTier(e.target.value as typeof tier); markDirty(); }}
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
      </FieldSection>

      {/* Tags */}
      <FieldSection label="Tags">
        <div className="flex flex-wrap gap-2 mb-2">
          {tags.map((tag) => (
            <span
              key={tag}
              className="inline-flex items-center gap-1 px-2.5 py-1 rounded-full text-[10px] font-mono font-bold"
              style={{
                background: "rgba(0, 240, 255, 0.08)",
                border: "1px solid rgba(0, 240, 255, 0.2)",
                color: "#00f0ff",
              }}
            >
              {tag}
              <button
                onClick={() => removeTag(tag)}
                className="ml-0.5 hover:text-red-400 transition-colors"
              >
                &times;
              </button>
            </span>
          ))}
        </div>
        <div className="flex gap-2">
          <input
            type="text"
            value={tagInput}
            onChange={(e) => setTagInput(e.target.value)}
            onKeyDown={(e) => { if (e.key === "Enter") { e.preventDefault(); addTag(); } }}
            className="flex-1 rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
            style={{
              background: "#0f1724",
              border: "1px solid rgba(0, 240, 255, 0.1)",
            }}
            placeholder="Add tag and press Enter"
          />
          <button
            onClick={addTag}
            className="px-3 py-2 rounded-lg text-xs font-bold tracking-wider uppercase transition-opacity hover:opacity-80"
            style={{
              background: "rgba(0, 240, 255, 0.08)",
              border: "1px solid rgba(0, 240, 255, 0.2)",
              color: "#00f0ff",
            }}
          >
            Add
          </button>
        </div>
      </FieldSection>

      {/* Skills */}
      <FieldSection label={`Skills (${skills.length})`}>
        {skills.length > 0 && (
          <div className="space-y-2 mb-3">
            {skills.map((skill, i) => (
              <div
                key={i}
                className="flex items-start justify-between px-3 py-2.5 rounded-lg"
                style={{
                  background: "rgba(0, 240, 255, 0.03)",
                  border: "1px solid rgba(0, 240, 255, 0.08)",
                }}
              >
                <div className="min-w-0 flex-1">
                  <div className="text-xs font-bold text-slate-300">{skill.name}</div>
                  <div className="text-[10px] font-mono text-slate-500 truncate mt-0.5">
                    {skill.prompt.length > 100 ? skill.prompt.slice(0, 100) + "..." : skill.prompt}
                  </div>
                </div>
                <button
                  onClick={() => removeSkill(i)}
                  className="text-slate-600 hover:text-red-400 transition-colors ml-2 shrink-0"
                >
                  &times;
                </button>
              </div>
            ))}
          </div>
        )}

        {showSkillForm ? (
          <div
            className="rounded-lg px-4 py-3 space-y-3"
            style={{
              background: "rgba(251, 191, 36, 0.03)",
              border: "1px solid rgba(251, 191, 36, 0.1)",
            }}
          >
            <input
              type="text"
              value={newSkillName}
              onChange={(e) => setNewSkillName(e.target.value)}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(0, 240, 255, 0.1)",
              }}
              placeholder="Skill name"
            />
            <textarea
              value={newSkillPrompt}
              onChange={(e) => setNewSkillPrompt(e.target.value)}
              rows={3}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono resize-y outline-none"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(0, 240, 255, 0.1)",
              }}
              placeholder="Skill prompt..."
            />
            <div className="flex gap-2">
              <button
                onClick={addSkill}
                disabled={!newSkillName.trim() || !newSkillPrompt.trim()}
                className="px-3 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-opacity hover:opacity-80 disabled:opacity-30"
                style={{
                  background: "rgba(251, 191, 36, 0.15)",
                  border: "1px solid rgba(251, 191, 36, 0.3)",
                  color: "#fbbf24",
                }}
              >
                Add Skill
              </button>
              <button
                onClick={() => { setShowSkillForm(false); setNewSkillName(""); setNewSkillPrompt(""); }}
                className="px-3 py-1.5 rounded-lg text-xs font-medium text-slate-500 hover:text-slate-300 transition-colors"
              >
                Cancel
              </button>
            </div>
          </div>
        ) : (
          <button
            onClick={() => setShowSkillForm(true)}
            className="px-3 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-opacity hover:opacity-80"
            style={{
              background: "rgba(0, 240, 255, 0.06)",
              border: "1px solid rgba(0, 240, 255, 0.12)",
              color: "#00f0ff",
            }}
          >
            + Add Skill
          </button>
        )}
      </FieldSection>

      {/* Action buttons */}
      <div className="flex items-center gap-3 pt-2">
        <button
          onClick={handleSave}
          disabled={saving || !dirty}
          className="px-5 py-2 rounded-lg text-sm font-bold tracking-wider uppercase transition-opacity hover:opacity-80 disabled:opacity-30"
          style={{
            background: "rgba(251, 191, 36, 0.15)",
            border: "1px solid rgba(251, 191, 36, 0.3)",
            color: "#fbbf24",
          }}
        >
          {saving ? "Saving..." : "Save Changes"}
        </button>
        <button
          onClick={handleReset}
          disabled={saving || !dirty}
          className="px-4 py-2 rounded-lg text-sm font-medium text-slate-500 hover:text-slate-300 transition-colors disabled:opacity-30"
          style={{
            border: "1px solid rgba(100, 116, 139, 0.15)",
          }}
        >
          Reset
        </button>
        {dirty && (
          <span className="text-[10px] font-mono text-amber-400 animate-pulse">
            Unsaved changes
          </span>
        )}
      </div>
    </div>
  );
}

function FieldSection({ label, children }: { label: string; children: React.ReactNode }) {
  return (
    <div>
      <label className="block text-[10px] font-bold uppercase tracking-[0.15em] text-slate-500 mb-2">
        {label}
      </label>
      {children}
    </div>
  );
}
