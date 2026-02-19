"use client";

import { useEffect, useState, useMemo, useCallback } from "react";
import Link from "next/link";
import {
  fetchSkills,
  fetchSkill,
  deleteSkill,
  type SkillInfo,
  type SkillDetail,
} from "@/lib/api";
import CreateSkillModal from "./CreateSkillModal";

export default function SkillsTab() {
  const [skills, setSkills] = useState<SkillInfo[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [search, setSearch] = useState("");
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [expandedDetails, setExpandedDetails] = useState<Map<string, SkillDetail>>(new Map());
  const [refreshKey, setRefreshKey] = useState(0);

  // Modal state
  const [modalOpen, setModalOpen] = useState(false);
  const [editingSkill, setEditingSkill] = useState<SkillDetail | null>(null);

  // Delete confirmation
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  const [deleteInfo, setDeleteInfo] = useState<{ usedBy: number } | null>(null);
  const [deleting, setDeleting] = useState(false);

  // Fetch skills
  useEffect(() => {
    setLoading(true);
    setError(null);
    fetchSkills()
      .then(setSkills)
      .catch((e) => setError(e instanceof Error ? e.message : "Failed to load skills"))
      .finally(() => setLoading(false));
  }, [refreshKey]);

  const filtered = useMemo(() => {
    if (!search) return skills;
    const q = search.toLowerCase();
    return skills.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.description.toLowerCase().includes(q) ||
        s.tags.some((t) => t.toLowerCase().includes(q))
    );
  }, [skills, search]);

  const toggleExpand = useCallback(async (name: string) => {
    setExpanded((prev) => {
      const next = new Set(prev);
      if (next.has(name)) {
        next.delete(name);
      } else {
        next.add(name);
        // Fetch full detail if not cached
        if (!expandedDetails.has(name)) {
          fetchSkill(name).then((detail) => {
            setExpandedDetails((prev) => new Map(prev).set(name, detail));
          });
        }
      }
      return next;
    });
  }, [expandedDetails]);

  const handleEdit = useCallback(async (name: string) => {
    try {
      const detail = await fetchSkill(name);
      setEditingSkill(detail);
      setModalOpen(true);
    } catch {
      // Silently fail — user can retry
    }
  }, []);

  const handleDeleteClick = useCallback(async (name: string) => {
    setDeleteTarget(name);
    setDeleteInfo(null);
    try {
      const detail = await fetchSkill(name);
      setDeleteInfo({ usedBy: detail.used_by.length });
    } catch {
      setDeleteInfo({ usedBy: 0 });
    }
  }, []);

  const confirmDelete = useCallback(async () => {
    if (!deleteTarget) return;
    setDeleting(true);
    try {
      await deleteSkill(deleteTarget);
      setDeleteTarget(null);
      setRefreshKey((k) => k + 1);
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to delete skill");
    } finally {
      setDeleting(false);
    }
  }, [deleteTarget]);

  const handleSaved = useCallback(() => {
    setRefreshKey((k) => k + 1);
    setEditingSkill(null);
  }, []);

  const openCreate = useCallback(() => {
    setEditingSkill(null);
    setModalOpen(true);
  }, []);

  if (loading) {
    return (
      <div className="space-y-3">
        {Array.from({ length: 6 }).map((_, i) => (
          <div
            key={i}
            className="h-12 rounded-lg animate-pulse"
            style={{ background: "rgba(0, 240, 255, 0.03)" }}
          />
        ))}
      </div>
    );
  }

  if (error) {
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
    <div className="space-y-4">
      {/* Toolbar */}
      <div className="flex items-center gap-3">
        <input
          type="text"
          placeholder="Search skills..."
          value={search}
          onChange={(e) => setSearch(e.target.value)}
          className="px-3 py-1.5 rounded-lg text-xs font-mono w-64 outline-none placeholder:text-slate-600"
          style={{
            background: "rgba(0, 240, 255, 0.04)",
            border: "1px solid rgba(0, 240, 255, 0.1)",
            color: "#e2e8f0",
          }}
        />
        <span className="text-[10px] text-slate-600 font-mono">
          {filtered.length} skill{filtered.length !== 1 ? "s" : ""}
        </span>
        <button
          onClick={openCreate}
          className="ml-auto px-3 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase hover:opacity-80 transition-opacity"
          style={{
            background: "rgba(168, 85, 247, 0.12)",
            border: "1px solid rgba(168, 85, 247, 0.3)",
            color: "#a855f7",
          }}
        >
          + Create Skill
        </button>
      </div>

      {/* Table */}
      {filtered.length === 0 ? (
        <div className="py-12 text-center">
          <div className="text-slate-500 text-sm mb-1">No skills found</div>
          <div className="text-slate-600 text-xs font-mono">
            {search ? "Try a different search term" : "Create your first skill to get started."}
          </div>
        </div>
      ) : (
        <div
          className="rounded-xl overflow-hidden"
          style={{
            background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
            border: "1px solid rgba(0, 240, 255, 0.06)",
          }}
        >
          {/* Header */}
          <div
            className="grid grid-cols-[1fr_2fr_auto_auto] px-4 py-2.5 text-[10px] uppercase tracking-wider text-slate-500 font-medium"
            style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
          >
            <div>Skill Name</div>
            <div>Description</div>
            <div>Tags</div>
            <div className="text-right pr-1">Actions</div>
          </div>

          {/* Rows */}
          {filtered.map((skill) => {
            const isExpanded = expanded.has(skill.name);
            const detail = expandedDetails.get(skill.name);
            return (
              <div
                key={skill.name}
                style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.04)" }}
              >
                <div className="grid grid-cols-[1fr_2fr_auto_auto] px-4 py-2.5 items-center">
                  {/* Name (clickable) */}
                  <button
                    onClick={() => toggleExpand(skill.name)}
                    className="text-left text-xs font-mono font-bold hover:opacity-80 transition-opacity flex items-center gap-1.5"
                    style={{ color: "#a855f7" }}
                  >
                    <span
                      className="text-[10px] text-slate-600 transition-transform"
                      style={{
                        display: "inline-block",
                        transform: isExpanded ? "rotate(90deg)" : "rotate(0deg)",
                      }}
                    >
                      &#9656;
                    </span>
                    {skill.name}
                  </button>

                  {/* Description */}
                  <div className="text-[11px] text-slate-400 font-mono truncate pr-4">
                    {skill.description || "—"}
                  </div>

                  {/* Tags */}
                  <div className="flex flex-wrap gap-1 pr-4">
                    {skill.tags.map((tag) => (
                      <span
                        key={tag}
                        className="text-[9px] font-mono px-1.5 py-0.5 rounded"
                        style={{
                          background: "rgba(251, 191, 36, 0.06)",
                          color: "#fbbf24",
                          border: "1px solid rgba(251, 191, 36, 0.1)",
                        }}
                      >
                        {tag}
                      </span>
                    ))}
                  </div>

                  {/* Actions */}
                  <div className="flex items-center gap-1.5">
                    <button
                      onClick={() => handleEdit(skill.name)}
                      className="text-[10px] font-mono px-2 py-1 rounded hover:opacity-80 transition-opacity"
                      style={{
                        background: "rgba(0, 240, 255, 0.06)",
                        color: "#00f0ff",
                        border: "1px solid rgba(0, 240, 255, 0.1)",
                      }}
                    >
                      Edit
                    </button>
                    <button
                      onClick={() => handleDeleteClick(skill.name)}
                      className="text-[10px] font-mono px-2 py-1 rounded hover:opacity-80 transition-opacity"
                      style={{
                        background: "rgba(255, 71, 87, 0.06)",
                        color: "#ff4757",
                        border: "1px solid rgba(255, 71, 87, 0.1)",
                      }}
                    >
                      Delete
                    </button>
                  </div>
                </div>

                {/* Expanded detail */}
                {isExpanded && (
                  <div
                    className="px-4 pb-3 pt-0"
                    style={{ paddingLeft: "calc(1rem + 14px)" }}
                  >
                    {detail ? (
                      <div className="space-y-2">
                        <pre
                          className="text-[11px] text-slate-400 font-mono whitespace-pre-wrap rounded-lg p-3"
                          style={{
                            background: "rgba(0, 0, 0, 0.3)",
                            border: "1px solid rgba(168, 85, 247, 0.06)",
                            maxHeight: "200px",
                            overflowY: "auto",
                          }}
                        >
                          {detail.prompt}
                        </pre>
                        {detail.used_by.length > 0 && (
                          <div className="flex items-center gap-2">
                            <span className="text-[9px] text-slate-500 uppercase tracking-wider">
                              Used by:
                            </span>
                            <div className="flex flex-wrap gap-1">
                              {detail.used_by.map((name) => (
                                <Link
                                  key={name}
                                  href={`/bundeswehr/${encodeURIComponent(name)}`}
                                  className="text-[10px] font-mono px-1.5 py-0.5 rounded hover:opacity-80 transition-opacity"
                                  style={{
                                    background: "rgba(0, 240, 255, 0.06)",
                                    color: "#00f0ff",
                                    border: "1px solid rgba(0, 240, 255, 0.1)",
                                  }}
                                >
                                  {name}
                                </Link>
                              ))}
                            </div>
                          </div>
                        )}
                      </div>
                    ) : (
                      <div className="text-[10px] text-slate-600 font-mono animate-pulse">
                        Loading...
                      </div>
                    )}
                  </div>
                )}
              </div>
            );
          })}
        </div>
      )}

      {/* Create/Edit Modal */}
      <CreateSkillModal
        isOpen={modalOpen}
        onClose={() => {
          setModalOpen(false);
          setEditingSkill(null);
        }}
        onSaved={handleSaved}
        editingSkill={editingSkill}
      />

      {/* Delete Confirmation Dialog */}
      {deleteTarget && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center p-4"
          style={{ background: "rgba(0, 0, 0, 0.7)", backdropFilter: "blur(4px)" }}
          onClick={() => setDeleteTarget(null)}
        >
          <div
            className="w-full max-w-sm rounded-xl p-6"
            style={{
              background: "#111827",
              border: "1px solid rgba(255, 71, 87, 0.2)",
              boxShadow: "0 0 40px rgba(255, 71, 87, 0.06)",
            }}
            onClick={(e) => e.stopPropagation()}
          >
            <div className="text-sm font-bold mb-3" style={{ color: "#ff4757" }}>
              Delete Skill
            </div>
            <p className="text-xs text-slate-400 mb-1">
              Are you sure you want to delete <strong className="text-slate-200">{deleteTarget}</strong>?
            </p>
            {deleteInfo && deleteInfo.usedBy > 0 && (
              <p
                className="text-xs font-mono mb-4 px-2 py-1.5 rounded"
                style={{
                  background: "rgba(255, 71, 87, 0.06)",
                  color: "#ff4757",
                  border: "1px solid rgba(255, 71, 87, 0.1)",
                }}
              >
                This skill is used by {deleteInfo.usedBy} agent{deleteInfo.usedBy !== 1 ? "s" : ""}.
                Delete anyway?
              </p>
            )}
            {!deleteInfo && (
              <p className="text-[10px] text-slate-600 font-mono animate-pulse mb-4">
                Checking usage...
              </p>
            )}
            <div className="flex items-center justify-end gap-3">
              <button
                onClick={() => setDeleteTarget(null)}
                className="px-3 py-1.5 rounded-lg text-xs text-slate-500 hover:text-slate-300 transition-colors"
                style={{ border: "1px solid rgba(100, 116, 139, 0.15)" }}
              >
                Cancel
              </button>
              <button
                onClick={confirmDelete}
                disabled={deleting || !deleteInfo}
                className="px-4 py-1.5 rounded-lg text-xs font-bold tracking-wider uppercase transition-opacity hover:opacity-80 disabled:opacity-30"
                style={{
                  background: "rgba(255, 71, 87, 0.15)",
                  border: "1px solid rgba(255, 71, 87, 0.3)",
                  color: "#ff4757",
                }}
              >
                {deleting ? "Deleting..." : "Delete"}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
