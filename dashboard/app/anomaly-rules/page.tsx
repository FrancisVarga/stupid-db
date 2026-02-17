"use client";

import { useEffect, useState, useCallback } from "react";
import Link from "next/link";
import RuleSidebar from "@/components/db/RuleSidebar";
import RuleForm from "@/components/db/RuleForm";
import RuleDetailView from "@/components/db/RuleDetailView";
import {
  getRule,
  deleteRule,
  toggleRule,
  type RuleDocument,
  type RuleKind,
} from "@/lib/api-rules";

type PageMode = "view" | "add" | "edit";

export default function RulesPage() {
  const [selectedRuleId, setSelectedRuleId] = useState<string | null>(null);
  const [selectedRule, setSelectedRule] = useState<RuleDocument | null>(null);
  const [mode, setMode] = useState<PageMode>("view");
  const [sidebarKey, setSidebarKey] = useState(0);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [toggling, setToggling] = useState(false);
  const [deleting, setDeleting] = useState(false);

  const refresh = useCallback(() => {
    setSidebarKey((k) => k + 1);
  }, []);

  const loadRule = useCallback((id: string) => {
    setLoading(true);
    setError(null);
    getRule(id)
      .then((rule) => {
        setSelectedRule(rule);
        setLoading(false);
      })
      .catch((e) => {
        setError((e as Error).message);
        setLoading(false);
      });
  }, []);

  // Load rule detail when selected
  useEffect(() => {
    if (selectedRuleId && mode === "view") {
      loadRule(selectedRuleId);
    }
  }, [selectedRuleId, mode, loadRule]);

  const handleSelectRule = useCallback((id: string) => {
    setSelectedRuleId(id);
    setMode("view");
  }, []);

  const handleNewRule = useCallback(() => {
    setSelectedRuleId(null);
    setSelectedRule(null);
    setMode("add");
  }, []);

  const handleFormSave = useCallback(() => {
    setMode("view");
    refresh();
    if (selectedRuleId) {
      loadRule(selectedRuleId);
    }
  }, [selectedRuleId, refresh, loadRule]);

  const handleFormCancel = useCallback(() => {
    setMode("view");
  }, []);

  const handleToggle = async () => {
    if (!selectedRuleId) return;
    setToggling(true);
    setError(null);
    try {
      await toggleRule(selectedRuleId);
      refresh();
      loadRule(selectedRuleId);
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setToggling(false);
    }
  };

  const handleDelete = async () => {
    if (!selectedRuleId || !selectedRule) return;
    if (!confirm(`Delete rule "${selectedRule.metadata.name}"? This cannot be undone.`)) return;
    setDeleting(true);
    setError(null);
    try {
      await deleteRule(selectedRuleId);
      setSelectedRuleId(null);
      setSelectedRule(null);
      setMode("view");
      refresh();
    } catch (e) {
      setError((e as Error).message);
    } finally {
      setDeleting(false);
    }
  };

  return (
    <div className="h-screen flex flex-col">
      {/* Header */}
      <header
        className="px-6 py-3 flex items-center justify-between shrink-0"
        style={{
          borderBottom: "1px solid rgba(249, 115, 22, 0.08)",
          background: "linear-gradient(180deg, rgba(249, 115, 22, 0.02) 0%, transparent 100%)",
        }}
      >
        <div className="flex items-center gap-4">
          <Link
            href="/"
            className="text-slate-500 hover:text-slate-300 text-sm font-mono transition-colors"
          >
            &larr; Dashboard
          </Link>
          <div className="w-[1px] h-4" style={{ background: "rgba(249, 115, 22, 0.12)" }} />
          <h1 className="text-lg font-bold tracking-wider" style={{ color: "#f97316" }}>
            Rule Manager
          </h1>
        </div>
        <button
          onClick={handleNewRule}
          className="px-3 py-1.5 rounded-lg text-[10px] font-bold uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            background: "rgba(249, 115, 22, 0.1)",
            border: "1px solid rgba(249, 115, 22, 0.3)",
            color: "#f97316",
          }}
        >
          + New Rule
        </button>
      </header>

      {/* Body: sidebar + main */}
      <div className="flex-1 flex min-h-0">
        <div style={{ width: 280 }} className="shrink-0">
          <RuleSidebar
            selectedRuleId={selectedRuleId}
            onSelectRule={handleSelectRule}
            onNewRule={handleNewRule}
            refreshKey={sidebarKey}
          />
        </div>

        <div className="flex-1 overflow-y-auto px-8 py-6">
          {/* Error banner */}
          {error && (
            <div
              className="flex items-center gap-3 px-4 py-2.5 rounded-lg mb-5"
              style={{
                background: "rgba(255, 71, 87, 0.06)",
                border: "1px solid rgba(255, 71, 87, 0.15)",
              }}
            >
              <span className="w-2 h-2 rounded-full shrink-0 animate-pulse" style={{ background: "#ff4757" }} />
              <span className="text-xs text-red-400 font-medium">{error}</span>
              <button
                onClick={() => setError(null)}
                className="ml-auto text-red-400/50 hover:text-red-400 text-xs"
              >
                dismiss
              </button>
            </div>
          )}

          {/* Form mode */}
          {(mode === "add" || mode === "edit") && (
            <div className="mb-6">
              <RuleForm
                mode={mode}
                ruleId={mode === "edit" ? selectedRuleId ?? undefined : undefined}
                initialKind={selectedRule?.kind as RuleKind | undefined}
                onSave={handleFormSave}
                onCancel={handleFormCancel}
              />
            </div>
          )}

          {/* View mode: nothing selected */}
          {mode === "view" && !selectedRuleId && (
            <div className="flex flex-col items-center justify-center py-20">
              <svg width="48" height="48" viewBox="0 0 24 24" fill="none" stroke="#1e293b" strokeWidth="1.5" className="mb-4">
                <path d="M14 2H6a2 2 0 00-2 2v16a2 2 0 002 2h12a2 2 0 002-2V8z" />
                <path d="M14 2v6h6" />
                <path d="M16 13H8" />
                <path d="M16 17H8" />
              </svg>
              <p className="text-slate-500 text-sm font-mono mb-2">Select a rule or create a new one</p>
              <p className="text-slate-600 text-xs font-mono mb-4">
                Manage anomaly rules, entity schemas, feature configs, scoring, trends, and patterns
              </p>
              <button
                onClick={handleNewRule}
                className="px-4 py-2 rounded-lg text-xs font-bold uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  background: "rgba(249, 115, 22, 0.1)",
                  border: "1px solid rgba(249, 115, 22, 0.3)",
                  color: "#f97316",
                }}
              >
                + Create Your First Rule
              </button>
            </div>
          )}

          {/* View mode: loading selected rule */}
          {mode === "view" && selectedRuleId && loading && (
            <div className="flex items-center justify-center py-20">
              <span className="text-slate-600 text-sm font-mono animate-pulse">Loading rule...</span>
            </div>
          )}

          {/* View mode: rule detail */}
          {mode === "view" && selectedRule && !loading && (
            <RuleDetailView
              rule={selectedRule}
              onEdit={() => setMode("edit")}
              onDelete={handleDelete}
              onToggle={handleToggle}
              toggling={toggling}
              deleting={deleting}
              refreshKey={sidebarKey}
            />
          )}
        </div>
      </div>
    </div>
  );
}
