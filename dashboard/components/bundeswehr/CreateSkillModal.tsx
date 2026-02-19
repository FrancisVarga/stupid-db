"use client";

import { useState, useEffect, useCallback } from "react";
import { createSkill, updateSkill, type SkillDetail } from "@/lib/api";

interface CreateSkillModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSaved: () => void;
  editingSkill?: SkillDetail | null;
}

const NAME_RE = /^[a-z0-9][a-z0-9_-]*$/;

export default function CreateSkillModal({
  isOpen,
  onClose,
  onSaved,
  editingSkill,
}: CreateSkillModalProps) {
  const isEditing = !!editingSkill;

  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [prompt, setPrompt] = useState("");
  const [tagsInput, setTagsInput] = useState("");
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

  // Populate form
  useEffect(() => {
    if (isOpen) {
      if (editingSkill) {
        setName(editingSkill.name);
        setDescription(editingSkill.description);
        setPrompt(editingSkill.prompt);
        setTagsInput(editingSkill.tags.join(", "));
      } else {
        setName("");
        setDescription("");
        setPrompt("");
        setTagsInput("");
      }
      setError(null);
    }
  }, [isOpen, editingSkill]);

  const nameError =
    name.length > 0 && !NAME_RE.test(name)
      ? "Must be lowercase alphanumeric with hyphens/underscores"
      : null;

  const canSubmit = name.length > 0 && prompt.length > 0 && !nameError && !submitting;

  const handleSubmit = useCallback(async () => {
    if (!canSubmit) return;
    setSubmitting(true);
    setError(null);

    const tags = tagsInput
      .split(",")
      .map((t) => t.trim())
      .filter(Boolean);

    try {
      if (isEditing) {
        await updateSkill(editingSkill!.name, { name, description, prompt, tags });
      } else {
        await createSkill({ name, description, prompt, tags });
      }
      onSaved();
      onClose();
    } catch (e) {
      setError(e instanceof Error ? e.message : `Failed to ${isEditing ? "update" : "create"} skill`);
    } finally {
      setSubmitting(false);
    }
  }, [canSubmit, name, description, prompt, tagsInput, isEditing, editingSkill, onSaved, onClose]);

  if (!isOpen) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center p-4"
      style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
      onClick={onClose}
    >
      <div
        className="w-full max-w-xl rounded-2xl overflow-hidden max-h-[90vh] flex flex-col"
        style={{
          background: "#111827",
          border: "1px solid rgba(168, 85, 247, 0.2)",
          boxShadow: "0 0 60px rgba(168, 85, 247, 0.06), 0 25px 50px rgba(0, 0, 0, 0.5)",
        }}
        onClick={(e) => e.stopPropagation()}
      >
        {/* Header */}
        <div
          className="px-6 py-4 flex items-center justify-between shrink-0"
          style={{
            borderBottom: "1px solid rgba(168, 85, 247, 0.1)",
            background: "linear-gradient(180deg, rgba(168, 85, 247, 0.03) 0%, transparent 100%)",
          }}
        >
          <div className="flex items-center gap-3">
            <span className="text-sm font-bold tracking-wider" style={{ color: "#a855f7" }}>
              {isEditing ? "Edit Skill" : "Create Skill"}
            </span>
            <span
              className="text-[9px] font-mono font-bold uppercase tracking-wider px-1.5 py-0.5 rounded"
              style={{
                background: "rgba(168, 85, 247, 0.08)",
                color: "#a855f7",
                border: "1px solid rgba(168, 85, 247, 0.15)",
              }}
            >
              {isEditing ? "Edit" : "New"}
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
              disabled={isEditing}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1 disabled:opacity-50"
              style={{
                background: "#0f1724",
                border: `1px solid ${nameError ? "rgba(255, 71, 87, 0.3)" : "rgba(168, 85, 247, 0.15)"}`,
              }}
              placeholder="my-skill-name"
            />
            {nameError && (
              <p className="text-[10px] text-red-400 mt-1 font-mono">{nameError}</p>
            )}
          </Field>

          {/* Description */}
          <Field label="Description">
            <input
              type="text"
              value={description}
              onChange={(e) => setDescription(e.target.value)}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(168, 85, 247, 0.15)",
              }}
              placeholder="What does this skill do?"
            />
          </Field>

          {/* Prompt */}
          <Field label="Prompt" required>
            <textarea
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              rows={8}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono resize-y outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(168, 85, 247, 0.15)",
                minHeight: "160px",
              }}
              placeholder="The skill prompt..."
            />
          </Field>

          {/* Tags */}
          <Field label="Tags">
            <input
              type="text"
              value={tagsInput}
              onChange={(e) => setTagsInput(e.target.value)}
              className="w-full rounded-lg px-3 py-2 text-sm text-slate-200 font-mono outline-none focus:ring-1"
              style={{
                background: "#0f1724",
                border: "1px solid rgba(168, 85, 247, 0.15)",
              }}
              placeholder="tag1, tag2, tag3"
            />
            <p className="text-[10px] text-slate-600 font-mono mt-1">Comma-separated</p>
          </Field>
        </div>

        {/* Footer */}
        <div
          className="px-6 py-4 flex items-center justify-end gap-3 shrink-0"
          style={{
            borderTop: "1px solid rgba(168, 85, 247, 0.1)",
            background: "linear-gradient(0deg, rgba(168, 85, 247, 0.02) 0%, transparent 100%)",
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
              background: "rgba(168, 85, 247, 0.15)",
              border: "1px solid rgba(168, 85, 247, 0.3)",
              color: "#a855f7",
            }}
          >
            {submitting
              ? isEditing ? "Saving..." : "Creating..."
              : isEditing ? "Save Changes" : "Create Skill"}
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
        {required && <span className="text-purple-400 ml-1">*</span>}
      </label>
      {children}
    </div>
  );
}
