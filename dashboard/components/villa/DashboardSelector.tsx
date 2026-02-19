"use client";

import { useState, useRef, useEffect } from "react";
import { useVillaStore } from "@/lib/villa/store";

export default function DashboardSelector() {
  const dashboards = useVillaStore((s) => s.dashboards);
  const activeDashboardId = useVillaStore((s) => s.activeDashboardId);
  const createDashboard = useVillaStore((s) => s.createDashboard);
  const deleteDashboard = useVillaStore((s) => s.deleteDashboard);
  const switchDashboard = useVillaStore((s) => s.switchDashboard);
  const renameDashboard = useVillaStore((s) => s.renameDashboard);

  const [menuOpenId, setMenuOpenId] = useState<string | null>(null);
  const [renamingId, setRenamingId] = useState<string | null>(null);
  const [renameValue, setRenameValue] = useState("");
  const [isCreating, setIsCreating] = useState(false);
  const [newName, setNewName] = useState("");

  const renameInputRef = useRef<HTMLInputElement>(null);
  const createInputRef = useRef<HTMLInputElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);

  // Focus inputs when they appear
  useEffect(() => {
    if (renamingId) renameInputRef.current?.focus();
  }, [renamingId]);

  useEffect(() => {
    if (isCreating) createInputRef.current?.focus();
  }, [isCreating]);

  // Close menu on outside click
  useEffect(() => {
    if (!menuOpenId) return;
    const handler = (e: MouseEvent) => {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setMenuOpenId(null);
      }
    };
    document.addEventListener("mousedown", handler);
    return () => document.removeEventListener("mousedown", handler);
  }, [menuOpenId]);

  const handleRenameSubmit = () => {
    if (renamingId && renameValue.trim()) {
      renameDashboard(renamingId, renameValue.trim());
    }
    setRenamingId(null);
    setRenameValue("");
  };

  const handleCreateSubmit = () => {
    const name = newName.trim() || "New Dashboard";
    createDashboard(name);
    setIsCreating(false);
    setNewName("");
  };

  return (
    <div className="flex items-center gap-1">
      {dashboards.map((d) => (
        <div key={d.id} className="relative">
          {renamingId === d.id ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onBlur={handleRenameSubmit}
              onKeyDown={(e) => {
                if (e.key === "Enter") handleRenameSubmit();
                if (e.key === "Escape") {
                  setRenamingId(null);
                  setRenameValue("");
                }
              }}
              className="px-2 py-1 text-xs rounded bg-slate-800 text-slate-200 border border-cyan-500/40 outline-none w-28"
            />
          ) : (
            <div
              role="tab"
              tabIndex={0}
              onClick={() => switchDashboard(d.id)}
              onKeyDown={(e) => {
                if (e.key === "Enter" || e.key === " ") switchDashboard(d.id);
              }}
              onContextMenu={(e) => {
                e.preventDefault();
                setMenuOpenId(menuOpenId === d.id ? null : d.id);
              }}
              className={`px-3 py-1 text-xs rounded-md transition-colors cursor-pointer inline-flex items-center ${
                d.id === activeDashboardId
                  ? "bg-cyan-500/15 text-cyan-300 border border-cyan-500/30"
                  : "text-slate-400 hover:text-slate-200 hover:bg-slate-800/60 border border-transparent"
              }`}
            >
              {d.name}
              <button
                onClick={(e) => {
                  e.stopPropagation();
                  setMenuOpenId(menuOpenId === d.id ? null : d.id);
                }}
                className="ml-1.5 text-slate-500 hover:text-slate-300 inline-flex"
                aria-label={`Options for ${d.name}`}
              >
                ···
              </button>
            </div>
          )}

          {/* Context menu */}
          {menuOpenId === d.id && (
            <div
              ref={menuRef}
              className="absolute top-full left-0 mt-1 z-50 bg-slate-800 border border-slate-700 rounded-md shadow-lg py-1 min-w-[120px]"
            >
              <button
                onClick={() => {
                  setRenamingId(d.id);
                  setRenameValue(d.name);
                  setMenuOpenId(null);
                }}
                className="w-full text-left px-3 py-1.5 text-xs text-slate-300 hover:bg-slate-700"
              >
                Rename
              </button>
              {dashboards.length > 1 && (
                <button
                  onClick={() => {
                    deleteDashboard(d.id);
                    setMenuOpenId(null);
                  }}
                  className="w-full text-left px-3 py-1.5 text-xs text-red-400 hover:bg-slate-700"
                >
                  Delete
                </button>
              )}
            </div>
          )}
        </div>
      ))}

      {/* Create new dashboard */}
      {isCreating ? (
        <input
          ref={createInputRef}
          value={newName}
          onChange={(e) => setNewName(e.target.value)}
          onBlur={handleCreateSubmit}
          onKeyDown={(e) => {
            if (e.key === "Enter") handleCreateSubmit();
            if (e.key === "Escape") {
              setIsCreating(false);
              setNewName("");
            }
          }}
          placeholder="Dashboard name..."
          className="px-2 py-1 text-xs rounded bg-slate-800 text-slate-200 border border-cyan-500/40 outline-none w-32 placeholder:text-slate-600"
        />
      ) : (
        <button
          onClick={() => setIsCreating(true)}
          className="px-2 py-1 text-xs text-slate-500 hover:text-cyan-400 hover:bg-slate-800/60 rounded-md transition-colors border border-dashed border-slate-700 hover:border-cyan-500/30"
          title="Create new dashboard"
          aria-label="Create new dashboard"
        >
          +
        </button>
      )}
    </div>
  );
}
