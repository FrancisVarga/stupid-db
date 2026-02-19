"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import {
  fetchPrompts,
  fetchPrompt,
  updatePrompt,
  type PromptSummary,
  type PromptDetail,
} from "@/lib/api";

export default function PromptsPage() {
  const [prompts, setPrompts] = useState<PromptSummary[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<PromptDetail | null>(null);
  const [editContent, setEditContent] = useState("");
  const [saving, setSaving] = useState(false);
  const [saveMsg, setSaveMsg] = useState<string | null>(null);
  const [previewMode, setPreviewMode] = useState(false);

  useEffect(() => {
    fetchPrompts()
      .then(setPrompts)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load prompts"))
      .finally(() => setLoading(false));
  }, []);

  const selectPrompt = async (name: string) => {
    try {
      const detail = await fetchPrompt(name);
      setSelected(detail);
      setEditContent(detail.content);
      setSaveMsg(null);
      setPreviewMode(false);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to load prompt");
    }
  };

  const handleSave = async () => {
    if (!selected) return;
    setSaving(true);
    setSaveMsg(null);
    try {
      const updated = await updatePrompt(selected.name, editContent);
      setSelected(updated);
      // Refresh summary list
      const list = await fetchPrompts();
      setPrompts(list);
      setSaveMsg("Saved");
      setTimeout(() => setSaveMsg(null), 2000);
    } catch (e) {
      setSaveMsg(e instanceof Error ? e.message : "Save failed");
    } finally {
      setSaving(false);
    }
  };

  const previewContent = () => {
    if (!selected) return editContent;
    let preview = editContent;
    for (const ph of selected.placeholders) {
      preview = preview.replaceAll(`<<<${ph}>>>`, `[SAMPLE: ${ph}]`);
    }
    return preview;
  };

  const hasChanges = selected && editContent !== selected.content;

  return (
    <div className="min-h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center gap-3 shrink-0"
        style={{
          borderBottom: "1px solid rgba(251, 191, 36, 0.08)",
          background: "linear-gradient(180deg, rgba(251, 191, 36, 0.02) 0%, transparent 100%)",
        }}
      >
        <Link href="/bundeswehr" className="text-slate-500 hover:text-slate-300 text-xs">
          &larr; Bundeswehr
        </Link>
        <h1 className="text-lg font-bold tracking-wider" style={{ color: "#fbbf24" }}>
          Prompt Templates
        </h1>
        <span className="text-slate-500 text-xs tracking-widest uppercase">
          {prompts.length} template{prompts.length !== 1 ? "s" : ""}
        </span>
      </header>

      {/* Main content */}
      <div className="flex flex-1 overflow-hidden">
        {/* Sidebar: prompt list */}
        <div
          className="w-72 shrink-0 overflow-y-auto"
          style={{ borderRight: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          {loading && (
            <div className="p-4 text-xs text-slate-500">Loading...</div>
          )}
          {error && (
            <div className="p-4 text-xs text-red-400">{error}</div>
          )}
          {prompts.map((p) => (
            <button
              key={p.name}
              onClick={() => selectPrompt(p.name)}
              className="w-full text-left px-4 py-3 transition-colors"
              style={{
                borderBottom: "1px solid rgba(0, 240, 255, 0.04)",
                background:
                  selected?.name === p.name
                    ? "rgba(251, 191, 36, 0.08)"
                    : "transparent",
              }}
            >
              <div
                className="text-sm font-mono font-medium"
                style={{
                  color: selected?.name === p.name ? "#fbbf24" : "#e2e8f0",
                }}
              >
                {p.name}
              </div>
              {p.description && (
                <div className="text-xs text-slate-500 mt-0.5">{p.description}</div>
              )}
              <div className="flex items-center gap-2 mt-1">
                {p.placeholders.map((ph) => (
                  <span
                    key={ph}
                    className="text-[10px] px-1.5 py-0.5 rounded font-mono"
                    style={{
                      background: "rgba(168, 85, 247, 0.1)",
                      color: "#a855f7",
                    }}
                  >
                    {ph}
                  </span>
                ))}
                <span className="text-[10px] text-slate-600 ml-auto">
                  {new Date(p.updated_at).toLocaleDateString()}
                </span>
              </div>
            </button>
          ))}
        </div>

        {/* Editor area */}
        <div className="flex-1 flex flex-col overflow-hidden">
          {!selected ? (
            <div className="flex-1 flex items-center justify-center text-slate-600 text-sm">
              Select a prompt to view and edit
            </div>
          ) : (
            <>
              {/* Toolbar */}
              <div
                className="px-4 py-2 flex items-center gap-3 shrink-0"
                style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
              >
                <span className="text-sm font-mono font-medium" style={{ color: "#fbbf24" }}>
                  {selected.name}
                </span>
                <div className="flex items-center gap-1 ml-auto">
                  <button
                    onClick={() => setPreviewMode(!previewMode)}
                    className="px-3 py-1 rounded text-xs font-medium transition-colors"
                    style={{
                      background: previewMode
                        ? "rgba(6, 214, 160, 0.15)"
                        : "rgba(0, 240, 255, 0.06)",
                      color: previewMode ? "#06d6a0" : "#94a3b8",
                      border: `1px solid ${previewMode ? "rgba(6, 214, 160, 0.3)" : "rgba(0, 240, 255, 0.1)"}`,
                    }}
                  >
                    {previewMode ? "Edit" : "Preview"}
                  </button>
                  <button
                    onClick={handleSave}
                    disabled={saving || !hasChanges}
                    className="px-3 py-1 rounded text-xs font-medium transition-colors"
                    style={{
                      background: hasChanges
                        ? "rgba(251, 191, 36, 0.15)"
                        : "rgba(0, 240, 255, 0.04)",
                      color: hasChanges ? "#fbbf24" : "#475569",
                      border: `1px solid ${hasChanges ? "rgba(251, 191, 36, 0.3)" : "rgba(0, 240, 255, 0.06)"}`,
                      cursor: hasChanges ? "pointer" : "default",
                    }}
                  >
                    {saving ? "Saving..." : "Save"}
                  </button>
                  {saveMsg && (
                    <span
                      className="text-xs ml-2"
                      style={{ color: saveMsg === "Saved" ? "#06d6a0" : "#ef4444" }}
                    >
                      {saveMsg}
                    </span>
                  )}
                </div>
              </div>

              {/* Editor / Preview */}
              <div className="flex-1 overflow-y-auto p-4">
                {previewMode ? (
                  <pre
                    className="whitespace-pre-wrap text-sm font-mono leading-relaxed"
                    style={{ color: "#cbd5e1" }}
                  >
                    {previewContent()}
                  </pre>
                ) : (
                  <textarea
                    value={editContent}
                    onChange={(e) => setEditContent(e.target.value)}
                    className="w-full h-full min-h-[500px] p-3 rounded-lg text-sm font-mono leading-relaxed resize-none outline-none"
                    style={{
                      background: "rgba(0, 0, 0, 0.3)",
                      color: "#e2e8f0",
                      border: "1px solid rgba(0, 240, 255, 0.08)",
                    }}
                  />
                )}
              </div>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
