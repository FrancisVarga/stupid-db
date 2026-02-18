"use client";

import { useState, useCallback } from "react";

/* ─── Types ──────────────────────────────────── */

interface Skill {
  name: string;
  description: string;
  config: Record<string, unknown>;
}

interface SkillsConfig {
  skills: Skill[];
}

interface McpServer {
  name: string;
  command: string;
  args: string[];
  env: Record<string, string>;
}

interface McpServersConfig {
  servers: McpServer[];
}

interface Tool {
  name: string;
  enabled: boolean;
  definition?: Record<string, unknown>;
}

interface ToolsConfig {
  tools: Tool[];
}

interface SkillMcpManagerProps {
  skillsConfig: SkillsConfig;
  mcpServersConfig: McpServersConfig;
  toolsConfig: ToolsConfig;
  onSkillsChange: (config: SkillsConfig) => void;
  onMcpServersChange: (config: McpServersConfig) => void;
  onToolsChange: (config: ToolsConfig) => void;
}

/* ─── Shared styles ──────────────────────────── */

const inputStyle = {
  background: "rgba(255, 255, 255, 0.03)",
  border: "1px solid rgba(0, 240, 255, 0.08)",
  color: "#e2e8f0",
};

const sectionCardStyle = {
  background: "linear-gradient(135deg, #0c1018 0%, #111827 100%)",
  border: "1px solid rgba(0, 240, 255, 0.08)",
};

/* ─── Catalog of built-in skills ─────────────── */

const SKILL_CATALOG: Skill[] = [
  { name: "data-analysis", description: "Analyze datasets and produce summaries", config: {} },
  { name: "code-review", description: "Review code for quality and security issues", config: {} },
  { name: "summarizer", description: "Summarize long documents or conversations", config: {} },
  { name: "web-search", description: "Search the web for information", config: {} },
];

/* ─── Default tool list ──────────────────────── */

const DEFAULT_TOOLS: Tool[] = [
  { name: "bash", enabled: true },
  { name: "read", enabled: true },
  { name: "write", enabled: true },
  { name: "glob", enabled: true },
  { name: "grep", enabled: true },
  { name: "web_fetch", enabled: false },
  { name: "web_search", enabled: false },
];

/* ─── Main component ─────────────────────────── */

export default function SkillMcpManager({
  skillsConfig,
  mcpServersConfig,
  toolsConfig,
  onSkillsChange,
  onMcpServersChange,
  onToolsChange,
}: SkillMcpManagerProps) {
  const [activeSection, setActiveSection] = useState<
    "skills" | "mcp" | "tools"
  >("skills");

  const sections = [
    { key: "skills" as const, label: "Skills", color: "#06d6a0", count: skillsConfig.skills.length },
    { key: "mcp" as const, label: "MCP Servers", color: "#00f0ff", count: mcpServersConfig.servers.length },
    { key: "tools" as const, label: "Tools", color: "#a855f7", count: toolsConfig.tools.length },
  ];

  return (
    <div className="space-y-3">
      {/* Section tabs */}
      <div className="flex items-center gap-1">
        {sections.map((s) => {
          const isActive = activeSection === s.key;
          return (
            <button
              key={s.key}
              onClick={() => setActiveSection(s.key)}
              className="relative px-3 py-1.5 rounded-lg text-[10px] font-bold font-mono uppercase tracking-wider transition-all"
              style={{
                color: isActive ? s.color : "#475569",
                border: `1px solid ${isActive ? `${s.color}40` : "rgba(71, 85, 105, 0.15)"}`,
                background: isActive ? `${s.color}10` : "transparent",
              }}
            >
              {s.label}
              {s.count > 0 && (
                <span
                  className="ml-1.5 px-1 py-0.5 rounded text-[8px]"
                  style={{
                    background: `${s.color}20`,
                    color: s.color,
                  }}
                >
                  {s.count}
                </span>
              )}
            </button>
          );
        })}
      </div>

      {/* Section content */}
      {activeSection === "skills" && (
        <SkillsSection
          config={skillsConfig}
          onChange={onSkillsChange}
        />
      )}
      {activeSection === "mcp" && (
        <McpServersSection
          config={mcpServersConfig}
          onChange={onMcpServersChange}
        />
      )}
      {activeSection === "tools" && (
        <ToolsSection
          config={toolsConfig}
          onChange={onToolsChange}
        />
      )}
    </div>
  );
}

/* ─── Skills Section ─────────────────────────── */

function SkillsSection({
  config,
  onChange,
}: {
  config: SkillsConfig;
  onChange: (c: SkillsConfig) => void;
}) {
  const [showAdd, setShowAdd] = useState(false);
  const [showImport, setShowImport] = useState(false);
  const [importJson, setImportJson] = useState("");
  const [importError, setImportError] = useState<string | null>(null);
  const [previewIdx, setPreviewIdx] = useState<number | null>(null);

  const addFromCatalog = useCallback(
    (skill: Skill) => {
      if (config.skills.some((s) => s.name === skill.name)) return;
      onChange({ skills: [...config.skills, skill] });
      setShowAdd(false);
    },
    [config, onChange],
  );

  const removeSkill = useCallback(
    (idx: number) => {
      onChange({ skills: config.skills.filter((_, i) => i !== idx) });
      if (previewIdx === idx) setPreviewIdx(null);
    },
    [config, onChange, previewIdx],
  );

  const handleImport = useCallback(() => {
    try {
      const parsed = JSON.parse(importJson);
      const skill: Skill = {
        name: parsed.name ?? "unnamed-skill",
        description: parsed.description ?? "",
        config: parsed.config ?? parsed,
      };
      onChange({ skills: [...config.skills, skill] });
      setShowImport(false);
      setImportJson("");
      setImportError(null);
    } catch {
      setImportError("Invalid JSON");
    }
  }, [importJson, config, onChange]);

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span
          className="text-[9px] font-bold uppercase tracking-[0.15em] font-mono"
          style={{ color: "#06d6a0" }}
        >
          Configured Skills
        </span>
        <div className="flex gap-1.5">
          <button
            onClick={() => { setShowAdd(!showAdd); setShowImport(false); }}
            className="px-2 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#06d6a0",
              border: "1px solid rgba(6, 214, 160, 0.25)",
              background: "rgba(6, 214, 160, 0.06)",
            }}
          >
            + Catalog
          </button>
          <button
            onClick={() => { setShowImport(!showImport); setShowAdd(false); }}
            className="px-2 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#00f0ff",
              border: "1px solid rgba(0, 240, 255, 0.25)",
              background: "rgba(0, 240, 255, 0.06)",
            }}
          >
            + Import
          </button>
        </div>
      </div>

      {/* Catalog picker */}
      {showAdd && (
        <div className="rounded-lg p-3 space-y-2" style={sectionCardStyle}>
          <span className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
            Skill Catalog
          </span>
          <div className="grid grid-cols-2 gap-2">
            {SKILL_CATALOG.filter(
              (cs) => !config.skills.some((s) => s.name === cs.name),
            ).map((cs) => (
              <button
                key={cs.name}
                onClick={() => addFromCatalog(cs)}
                className="text-left px-3 py-2 rounded-lg transition-all hover:opacity-80"
                style={{
                  background: "rgba(6, 214, 160, 0.04)",
                  border: "1px solid rgba(6, 214, 160, 0.12)",
                }}
              >
                <div className="text-[10px] font-bold font-mono" style={{ color: "#06d6a0" }}>
                  {cs.name}
                </div>
                <div className="text-[9px] text-slate-500 font-mono mt-0.5">
                  {cs.description}
                </div>
              </button>
            ))}
            {SKILL_CATALOG.filter(
              (cs) => !config.skills.some((s) => s.name === cs.name),
            ).length === 0 && (
              <div className="col-span-2 text-[9px] text-slate-600 font-mono text-center py-2">
                All catalog skills already added
              </div>
            )}
          </div>
        </div>
      )}

      {/* Import JSON */}
      {showImport && (
        <div className="rounded-lg p-3 space-y-2" style={sectionCardStyle}>
          <span className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
            Import Skill Config (JSON)
          </span>
          <textarea
            value={importJson}
            onChange={(e) => { setImportJson(e.target.value); setImportError(null); }}
            rows={5}
            placeholder={'{\n  "name": "my-skill",\n  "description": "...",\n  "config": {}\n}'}
            className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none resize-y"
            style={inputStyle}
          />
          {importError && (
            <span className="text-[9px] text-red-400 font-mono">{importError}</span>
          )}
          <div className="flex gap-1.5">
            <button
              onClick={handleImport}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
              style={{
                color: "#06d6a0",
                border: "1px solid rgba(6, 214, 160, 0.3)",
                background: "rgba(6, 214, 160, 0.08)",
              }}
            >
              Add Skill
            </button>
            <button
              onClick={() => { setShowImport(false); setImportJson(""); setImportError(null); }}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
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
      )}

      {/* Skill list */}
      {config.skills.length === 0 ? (
        <div
          className="rounded-lg px-4 py-6 text-center"
          style={sectionCardStyle}
        >
          <p className="text-[10px] text-slate-500 font-mono">No skills configured</p>
          <p className="text-[9px] text-slate-600 font-mono mt-0.5">
            Add from the catalog or import a JSON config
          </p>
        </div>
      ) : (
        <div className="space-y-1.5">
          {config.skills.map((skill, idx) => (
            <div key={skill.name + idx}>
              <div
                className="flex items-center gap-3 px-3 py-2 rounded-lg"
                style={{
                  background: idx % 2 === 0 ? "rgba(15, 23, 42, 0.3)" : "rgba(15, 23, 42, 0.5)",
                  border: previewIdx === idx ? "1px solid rgba(6, 214, 160, 0.2)" : "1px solid transparent",
                }}
              >
                <span
                  className="w-1.5 h-1.5 rounded-full shrink-0"
                  style={{ background: "#06d6a0" }}
                />
                <div className="flex-1 min-w-0">
                  <div className="text-[11px] font-mono text-slate-200 truncate">
                    {skill.name}
                  </div>
                  <div className="text-[9px] font-mono text-slate-500 truncate">
                    {skill.description || "No description"}
                  </div>
                </div>
                <button
                  onClick={() => setPreviewIdx(previewIdx === idx ? null : idx)}
                  className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
                  style={{
                    color: "#00f0ff",
                    border: "1px solid rgba(0, 240, 255, 0.2)",
                    background: "rgba(0, 240, 255, 0.04)",
                  }}
                >
                  {previewIdx === idx ? "Hide" : "View"}
                </button>
                <button
                  onClick={() => removeSkill(idx)}
                  className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
                  style={{
                    color: "#ff4757",
                    border: "1px solid rgba(255, 71, 87, 0.2)",
                    background: "rgba(255, 71, 87, 0.04)",
                  }}
                >
                  Remove
                </button>
              </div>
              {/* Preview panel */}
              {previewIdx === idx && (
                <pre
                  className="mt-1 px-3 py-2 rounded-lg text-[10px] font-mono text-slate-400 overflow-x-auto"
                  style={{
                    background: "rgba(0, 0, 0, 0.3)",
                    border: "1px solid rgba(0, 240, 255, 0.06)",
                    maxHeight: 200,
                  }}
                >
                  {JSON.stringify(skill, null, 2)}
                </pre>
              )}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ─── MCP Servers Section ────────────────────── */

function McpServersSection({
  config,
  onChange,
}: {
  config: McpServersConfig;
  onChange: (c: McpServersConfig) => void;
}) {
  const [showForm, setShowForm] = useState(false);
  const [editIdx, setEditIdx] = useState<number | null>(null);
  const [name, setName] = useState("");
  const [command, setCommand] = useState("");
  const [args, setArgs] = useState("");
  const [envPairs, setEnvPairs] = useState("");

  const resetForm = useCallback(() => {
    setName("");
    setCommand("");
    setArgs("");
    setEnvPairs("");
    setShowForm(false);
    setEditIdx(null);
  }, []);

  const openEdit = useCallback((idx: number) => {
    const srv = config.servers[idx];
    setName(srv.name);
    setCommand(srv.command);
    setArgs(srv.args.join(" "));
    setEnvPairs(
      Object.entries(srv.env)
        .map(([k, v]) => `${k}=${v}`)
        .join("\n"),
    );
    setEditIdx(idx);
    setShowForm(true);
  }, [config]);

  const parseEnv = (raw: string): Record<string, string> => {
    const env: Record<string, string> = {};
    raw
      .split("\n")
      .filter((l) => l.includes("="))
      .forEach((l) => {
        const eqIdx = l.indexOf("=");
        env[l.slice(0, eqIdx).trim()] = l.slice(eqIdx + 1).trim();
      });
    return env;
  };

  const handleSave = useCallback(() => {
    if (!name.trim() || !command.trim()) return;
    const server: McpServer = {
      name: name.trim(),
      command: command.trim(),
      args: args
        .trim()
        .split(/\s+/)
        .filter(Boolean),
      env: parseEnv(envPairs),
    };

    if (editIdx !== null) {
      const updated = [...config.servers];
      updated[editIdx] = server;
      onChange({ servers: updated });
    } else {
      onChange({ servers: [...config.servers, server] });
    }
    resetForm();
  }, [name, command, args, envPairs, editIdx, config, onChange, resetForm]);

  const removeServer = useCallback(
    (idx: number) => {
      onChange({ servers: config.servers.filter((_, i) => i !== idx) });
    },
    [config, onChange],
  );

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span
          className="text-[9px] font-bold uppercase tracking-[0.15em] font-mono"
          style={{ color: "#00f0ff" }}
        >
          MCP Servers
        </span>
        {!showForm && (
          <button
            onClick={() => setShowForm(true)}
            className="px-2 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
            style={{
              color: "#00f0ff",
              border: "1px solid rgba(0, 240, 255, 0.25)",
              background: "rgba(0, 240, 255, 0.06)",
            }}
          >
            + Add Server
          </button>
        )}
      </div>

      {/* Add/Edit form */}
      {showForm && (
        <div className="rounded-lg p-4 space-y-3" style={sectionCardStyle}>
          <span
            className="text-[9px] font-bold uppercase tracking-wider font-mono"
            style={{ color: "#00f0ff" }}
          >
            {editIdx !== null ? "Edit MCP Server" : "New MCP Server"}
          </span>

          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Name
              </label>
              <input
                type="text"
                value={name}
                onChange={(e) => setName(e.target.value)}
                placeholder="postgres"
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
            <div className="space-y-1">
              <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
                Command
              </label>
              <input
                type="text"
                value={command}
                onChange={(e) => setCommand(e.target.value)}
                placeholder="npx"
                className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
                style={inputStyle}
              />
            </div>
          </div>

          <div className="space-y-1">
            <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
              Arguments (space-separated)
            </label>
            <input
              type="text"
              value={args}
              onChange={(e) => setArgs(e.target.value)}
              placeholder="-y @modelcontextprotocol/server-postgres"
              className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none"
              style={inputStyle}
            />
          </div>

          <div className="space-y-1">
            <label className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
              Environment Variables (KEY=VALUE, one per line)
            </label>
            <textarea
              value={envPairs}
              onChange={(e) => setEnvPairs(e.target.value)}
              rows={3}
              placeholder={"DATABASE_URL=postgresql://localhost:5432/mydb"}
              className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none resize-y"
              style={inputStyle}
            />
          </div>

          <div className="flex gap-1.5">
            <button
              onClick={handleSave}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
              style={{
                color: "#00f0ff",
                border: "1px solid rgba(0, 240, 255, 0.3)",
                background: "rgba(0, 240, 255, 0.08)",
              }}
            >
              {editIdx !== null ? "Update" : "Add"}
            </button>
            <button
              onClick={resetForm}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
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
      )}

      {/* Server list */}
      {config.servers.length === 0 ? (
        <div
          className="rounded-lg px-4 py-6 text-center"
          style={sectionCardStyle}
        >
          <p className="text-[10px] text-slate-500 font-mono">No MCP servers configured</p>
          <p className="text-[9px] text-slate-600 font-mono mt-0.5">
            Add a server to connect external tools
          </p>
        </div>
      ) : (
        <div className="space-y-1.5">
          {config.servers.map((srv, idx) => (
            <div
              key={srv.name + idx}
              className="flex items-center gap-3 px-3 py-2.5 rounded-lg"
              style={{
                background: idx % 2 === 0 ? "rgba(15, 23, 42, 0.3)" : "rgba(15, 23, 42, 0.5)",
              }}
            >
              <span
                className="w-1.5 h-1.5 rounded-full shrink-0"
                style={{ background: "#00f0ff" }}
              />
              <div className="flex-1 min-w-0">
                <div className="text-[11px] font-mono text-slate-200 truncate">
                  {srv.name}
                </div>
                <div className="text-[9px] font-mono text-slate-500 truncate">
                  {srv.command} {srv.args.join(" ")}
                </div>
              </div>

              {/* Transport badge */}
              <span
                className="px-1.5 py-0.5 rounded text-[8px] font-bold uppercase tracking-wider shrink-0"
                style={{
                  color: "#a855f7",
                  background: "rgba(168, 85, 247, 0.1)",
                  border: "1px solid rgba(168, 85, 247, 0.2)",
                }}
              >
                stdio
              </span>

              {/* Env count badge */}
              {Object.keys(srv.env).length > 0 && (
                <span
                  className="px-1.5 py-0.5 rounded text-[8px] font-bold font-mono shrink-0"
                  style={{
                    color: "#f97316",
                    background: "rgba(249, 115, 22, 0.1)",
                    border: "1px solid rgba(249, 115, 22, 0.2)",
                  }}
                >
                  {Object.keys(srv.env).length} env
                </span>
              )}

              <button
                onClick={() => openEdit(idx)}
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
                onClick={() => removeServer(idx)}
                className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  color: "#ff4757",
                  border: "1px solid rgba(255, 71, 87, 0.2)",
                  background: "rgba(255, 71, 87, 0.04)",
                }}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

/* ─── Tools Section ──────────────────────────── */

function ToolsSection({
  config,
  onChange,
}: {
  config: ToolsConfig;
  onChange: (c: ToolsConfig) => void;
}) {
  const [showCustom, setShowCustom] = useState(false);
  const [customJson, setCustomJson] = useState("");
  const [customError, setCustomError] = useState<string | null>(null);

  // Merge defaults with configured tools
  const mergedTools = DEFAULT_TOOLS.map((dt) => {
    const found = config.tools.find((t) => t.name === dt.name);
    return found ?? dt;
  });
  // Include any custom tools not in defaults
  const customTools = config.tools.filter(
    (t) => !DEFAULT_TOOLS.some((dt) => dt.name === t.name),
  );

  const toggleTool = useCallback(
    (name: string) => {
      const updated = config.tools.map((t) =>
        t.name === name ? { ...t, enabled: !t.enabled } : t,
      );
      // If tool wasn't in config yet, add it from defaults
      if (!config.tools.some((t) => t.name === name)) {
        const def = DEFAULT_TOOLS.find((dt) => dt.name === name);
        if (def) updated.push({ ...def, enabled: !def.enabled });
      }
      onChange({ tools: updated });
    },
    [config, onChange],
  );

  const removeTool = useCallback(
    (name: string) => {
      onChange({ tools: config.tools.filter((t) => t.name !== name) });
    },
    [config, onChange],
  );

  const handleAddCustom = useCallback(() => {
    try {
      const parsed = JSON.parse(customJson);
      const tool: Tool = {
        name: parsed.name ?? "custom-tool",
        enabled: true,
        definition: parsed.definition ?? parsed,
      };
      onChange({ tools: [...config.tools, tool] });
      setShowCustom(false);
      setCustomJson("");
      setCustomError(null);
    } catch {
      setCustomError("Invalid JSON");
    }
  }, [customJson, config, onChange]);

  return (
    <div className="space-y-3">
      {/* Header */}
      <div className="flex items-center justify-between">
        <span
          className="text-[9px] font-bold uppercase tracking-[0.15em] font-mono"
          style={{ color: "#a855f7" }}
        >
          Tools
        </span>
        <button
          onClick={() => setShowCustom(!showCustom)}
          className="px-2 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
          style={{
            color: "#a855f7",
            border: "1px solid rgba(168, 85, 247, 0.25)",
            background: "rgba(168, 85, 247, 0.06)",
          }}
        >
          + Custom Tool
        </button>
      </div>

      {/* Custom tool JSON editor */}
      {showCustom && (
        <div className="rounded-lg p-3 space-y-2" style={sectionCardStyle}>
          <span className="text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono">
            Custom Tool Definition (JSON)
          </span>
          <textarea
            value={customJson}
            onChange={(e) => { setCustomJson(e.target.value); setCustomError(null); }}
            rows={6}
            placeholder={'{\n  "name": "my-tool",\n  "definition": {\n    "description": "...",\n    "parameters": {}\n  }\n}'}
            className="w-full px-3 py-2 rounded-lg text-[11px] font-mono outline-none resize-y"
            style={inputStyle}
          />
          {customError && (
            <span className="text-[9px] text-red-400 font-mono">{customError}</span>
          )}
          <div className="flex gap-1.5">
            <button
              onClick={handleAddCustom}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
              style={{
                color: "#a855f7",
                border: "1px solid rgba(168, 85, 247, 0.3)",
                background: "rgba(168, 85, 247, 0.08)",
              }}
            >
              Add Tool
            </button>
            <button
              onClick={() => { setShowCustom(false); setCustomJson(""); setCustomError(null); }}
              className="px-3 py-1 rounded text-[9px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
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
      )}

      {/* Built-in tools grid */}
      <div className="rounded-lg overflow-hidden" style={sectionCardStyle}>
        <div
          className="px-3 py-2 text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono"
          style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
        >
          Built-in Tools
        </div>
        <div className="grid grid-cols-2 gap-0">
          {mergedTools.map((tool, idx) => (
            <div
              key={tool.name}
              className="flex items-center gap-2.5 px-3 py-2"
              style={{
                background: idx % 2 === 0 ? "rgba(15, 23, 42, 0.3)" : "rgba(15, 23, 42, 0.5)",
                borderBottom: "1px solid rgba(0, 240, 255, 0.03)",
              }}
            >
              {/* Toggle switch */}
              <button
                onClick={() => toggleTool(tool.name)}
                className="relative w-7 h-4 rounded-full shrink-0 transition-all"
                style={{
                  background: tool.enabled
                    ? "rgba(6, 214, 160, 0.3)"
                    : "rgba(71, 85, 105, 0.2)",
                  border: `1px solid ${tool.enabled ? "rgba(6, 214, 160, 0.4)" : "rgba(71, 85, 105, 0.3)"}`,
                }}
              >
                <span
                  className="absolute top-0.5 w-2.5 h-2.5 rounded-full transition-all"
                  style={{
                    background: tool.enabled ? "#06d6a0" : "#475569",
                    left: tool.enabled ? 12 : 2,
                  }}
                />
              </button>
              <span
                className="text-[11px] font-mono truncate"
                style={{ color: tool.enabled ? "#e2e8f0" : "#475569" }}
              >
                {tool.name}
              </span>
            </div>
          ))}
        </div>
      </div>

      {/* Custom tools */}
      {customTools.length > 0 && (
        <div className="rounded-lg overflow-hidden" style={sectionCardStyle}>
          <div
            className="px-3 py-2 text-[9px] font-bold uppercase tracking-wider text-slate-500 font-mono"
            style={{ borderBottom: "1px solid rgba(0, 240, 255, 0.06)" }}
          >
            Custom Tools
          </div>
          {customTools.map((tool, idx) => (
            <div
              key={tool.name + idx}
              className="flex items-center gap-2.5 px-3 py-2"
              style={{
                background: idx % 2 === 0 ? "rgba(15, 23, 42, 0.3)" : "rgba(15, 23, 42, 0.5)",
                borderBottom: "1px solid rgba(0, 240, 255, 0.03)",
              }}
            >
              <button
                onClick={() => toggleTool(tool.name)}
                className="relative w-7 h-4 rounded-full shrink-0 transition-all"
                style={{
                  background: tool.enabled
                    ? "rgba(168, 85, 247, 0.3)"
                    : "rgba(71, 85, 105, 0.2)",
                  border: `1px solid ${tool.enabled ? "rgba(168, 85, 247, 0.4)" : "rgba(71, 85, 105, 0.3)"}`,
                }}
              >
                <span
                  className="absolute top-0.5 w-2.5 h-2.5 rounded-full transition-all"
                  style={{
                    background: tool.enabled ? "#a855f7" : "#475569",
                    left: tool.enabled ? 12 : 2,
                  }}
                />
              </button>
              <span
                className="text-[11px] font-mono truncate flex-1"
                style={{ color: tool.enabled ? "#e2e8f0" : "#475569" }}
              >
                {tool.name}
              </span>
              <button
                onClick={() => removeTool(tool.name)}
                className="px-2 py-0.5 rounded text-[8px] font-bold font-mono uppercase tracking-wider transition-all hover:opacity-80"
                style={{
                  color: "#ff4757",
                  border: "1px solid rgba(255, 71, 87, 0.2)",
                  background: "rgba(255, 71, 87, 0.04)",
                }}
              >
                Remove
              </button>
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
