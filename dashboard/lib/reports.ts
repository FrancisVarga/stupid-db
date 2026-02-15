/**
 * Report storage using localStorage.
 * Reports are saved conversations with inline visualizations.
 */

export interface RenderBlock {
  type:
    | "bar_chart"
    | "line_chart"
    | "scatter"
    | "force_graph"
    | "sankey"
    | "heatmap"
    | "treemap"
    | "table"
    | "summary";
  title: string;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  data: any;
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  config?: Record<string, any>;
}

export interface ReportMessage {
  id: string;
  role: "user" | "system";
  content: string;
  timestamp: string;
  renderBlocks?: RenderBlock[];
  suggestions?: string[];
}

export interface Report {
  id: string;
  title: string;
  messages: ReportMessage[];
  created_at: string;
  updated_at: string;
}

const STORAGE_KEY = "stupid-db-reports";

function readAll(): Report[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}

function writeAll(reports: Report[]) {
  if (typeof window === "undefined") return;
  localStorage.setItem(STORAGE_KEY, JSON.stringify(reports));
}

export function saveReport(
  messages: ReportMessage[],
  title?: string
): Report {
  const id = crypto.randomUUID();
  const now = new Date().toISOString();
  const autoTitle =
    title ||
    messages.find((m) => m.role === "user")?.content.slice(0, 60) ||
    "Untitled Report";

  const report: Report = {
    id,
    title: autoTitle,
    messages,
    created_at: now,
    updated_at: now,
  };

  const all = readAll();
  all.unshift(report);
  writeAll(all);

  return report;
}

export function getReport(id: string): Report | null {
  const all = readAll();
  return all.find((r) => r.id === id) ?? null;
}

export function listReports(): Report[] {
  return readAll();
}

export function deleteReport(id: string): void {
  const all = readAll();
  writeAll(all.filter((r) => r.id !== id));
}

// ── Query History ──────────────────────────────────────────

const HISTORY_KEY = "stupid-db-query-history";
const MAX_HISTORY = 50;

export interface QueryHistoryItem {
  question: string;
  timestamp: string;
}

export function saveQueryHistory(question: string) {
  if (typeof window === "undefined") return;
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    const history: QueryHistoryItem[] = raw ? JSON.parse(raw) : [];

    // Avoid duplicates of the exact same question
    const filtered = history.filter((h) => h.question !== question);
    filtered.unshift({
      question,
      timestamp: new Date().toISOString(),
    });

    // Trim to max
    if (filtered.length > MAX_HISTORY) {
      filtered.length = MAX_HISTORY;
    }

    localStorage.setItem(HISTORY_KEY, JSON.stringify(filtered));
  } catch {
    // Ignore storage errors
  }
}

export function loadQueryHistory(): QueryHistoryItem[] {
  if (typeof window === "undefined") return [];
  try {
    const raw = localStorage.getItem(HISTORY_KEY);
    return raw ? JSON.parse(raw) : [];
  } catch {
    return [];
  }
}
