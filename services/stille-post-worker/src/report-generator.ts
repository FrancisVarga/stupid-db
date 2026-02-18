import type postgres from "postgres";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

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
  data: unknown;
  config?: Record<string, unknown>;
}

export interface AgentOutput {
  agentName: string;
  output: string;
}

export interface GeneratedReport {
  id: string;
  title: string;
  contentHtml: string;
  contentJson: Record<string, unknown>;
  renderBlocks: RenderBlock[];
}

// ---------------------------------------------------------------------------
// Render block extraction
// ---------------------------------------------------------------------------

/** Known chart types that map to D3 dashboard components. */
const VALID_BLOCK_TYPES = new Set<RenderBlock["type"]>([
  "bar_chart",
  "line_chart",
  "scatter",
  "force_graph",
  "sankey",
  "heatmap",
  "treemap",
  "table",
  "summary",
]);

/**
 * Extract JSON code blocks from agent output and identify render blocks.
 *
 * Agents may emit fenced JSON blocks like:
 *
 * ```json
 * { "type": "bar_chart", "title": "Login failures by hour", "data": [...] }
 * ```
 *
 * We parse every JSON block and keep those whose `type` field matches a known
 * render block type. Non-matching JSON is ignored — agents may produce JSON
 * for other purposes (debugging, raw data dumps, etc.).
 */
export function parseRenderBlocks(agentOutputs: AgentOutput[]): RenderBlock[] {
  const blocks: RenderBlock[] = [];

  for (const { output } of agentOutputs) {
    // Match fenced code blocks (```json ... ``` or ``` ... ```)
    const codeBlockRe = /```(?:json)?\s*\n([\s\S]*?)```/g;
    let match: RegExpExecArray | null;

    while ((match = codeBlockRe.exec(output)) !== null) {
      const raw = match[1].trim();
      try {
        const parsed = JSON.parse(raw) as Record<string, unknown>;

        if (isRenderBlock(parsed)) {
          blocks.push(parsed as unknown as RenderBlock);
        } else if (Array.isArray(parsed)) {
          // Agent might emit an array of render blocks
          for (const item of parsed) {
            if (isRenderBlock(item as Record<string, unknown>)) {
              blocks.push(item as unknown as RenderBlock);
            }
          }
        }
      } catch {
        // Not valid JSON — skip
      }
    }
  }

  return blocks;
}

function isRenderBlock(obj: Record<string, unknown>): boolean {
  return (
    typeof obj.type === "string" &&
    VALID_BLOCK_TYPES.has(obj.type as RenderBlock["type"]) &&
    typeof obj.title === "string" &&
    obj.data !== undefined
  );
}

// ---------------------------------------------------------------------------
// HTML generation
// ---------------------------------------------------------------------------

/**
 * Generate a self-contained HTML report with inline CSS.
 * Designed for email delivery — no external stylesheets or scripts.
 */
export function generateHtml(
  pipelineName: string,
  agentOutputs: AgentOutput[],
  renderBlocks: RenderBlock[],
): string {
  const generatedAt = new Date().toLocaleString("en-US", {
    dateStyle: "long",
    timeStyle: "short",
  });

  const agentSections = agentOutputs
    .map(
      ({ agentName, output }) => `
    <div class="section">
      <h2>${escapeHtml(agentName)}</h2>
      <div class="content">${markdownToHtml(output)}</div>
    </div>`,
    )
    .join("\n");

  const blockSummary =
    renderBlocks.length > 0
      ? `
    <div class="section">
      <h2>Visualizations</h2>
      <p>${renderBlocks.length} interactive chart(s) available in the dashboard:</p>
      <ul>
        ${renderBlocks.map((b) => `<li><strong>${escapeHtml(b.title)}</strong> (${escapeHtml(b.type.replace(/_/g, " "))})</li>`).join("\n        ")}
      </ul>
    </div>`
      : "";

  return `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8">
  <meta name="viewport" content="width=device-width, initial-scale=1">
  <title>${escapeHtml(pipelineName)} Report</title>
  <style>
    body {
      font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, sans-serif;
      max-width: 720px;
      margin: 0 auto;
      padding: 24px;
      color: #1a1a2e;
      background: #f8f9fa;
      line-height: 1.6;
    }
    .header {
      border-bottom: 3px solid #e94560;
      padding-bottom: 16px;
      margin-bottom: 24px;
    }
    .header h1 {
      margin: 0 0 4px;
      font-size: 24px;
      color: #0f3460;
    }
    .header .meta {
      font-size: 13px;
      color: #6c757d;
    }
    .section {
      background: #fff;
      border-radius: 8px;
      padding: 20px 24px;
      margin-bottom: 16px;
      box-shadow: 0 1px 3px rgba(0,0,0,0.08);
    }
    .section h2 {
      margin: 0 0 12px;
      font-size: 18px;
      color: #16213e;
      border-bottom: 1px solid #eee;
      padding-bottom: 8px;
    }
    .content p { margin: 8px 0; }
    .content ul, .content ol { padding-left: 20px; }
    .content strong { color: #e94560; }
    .content code {
      background: #f1f3f5;
      padding: 2px 6px;
      border-radius: 3px;
      font-size: 0.9em;
    }
    .content pre {
      background: #1a1a2e;
      color: #e6e6e6;
      padding: 12px 16px;
      border-radius: 6px;
      overflow-x: auto;
      font-size: 0.85em;
    }
    table {
      width: 100%;
      border-collapse: collapse;
      margin: 12px 0;
    }
    th, td {
      text-align: left;
      padding: 8px 12px;
      border-bottom: 1px solid #dee2e6;
    }
    th { background: #f1f3f5; font-weight: 600; }
    .footer {
      text-align: center;
      font-size: 12px;
      color: #adb5bd;
      margin-top: 32px;
      padding-top: 16px;
      border-top: 1px solid #dee2e6;
    }
  </style>
</head>
<body>
  <div class="header">
    <h1>${escapeHtml(pipelineName)} Report</h1>
    <div class="meta">Generated ${escapeHtml(generatedAt)}</div>
  </div>
  ${agentSections}
  ${blockSummary}
  <div class="footer">
    Generated by Stille Post &mdash; stupid-db AI Pipeline
  </div>
</body>
</html>`;
}

// ---------------------------------------------------------------------------
// Markdown → HTML (lightweight, no dependencies)
// ---------------------------------------------------------------------------

/** Minimal markdown-to-HTML for agent output (headings, bold, lists, code). */
function markdownToHtml(md: string): string {
  const lines = md.split("\n");
  const htmlLines: string[] = [];
  let inList = false;
  let inCodeBlock = false;

  for (const line of lines) {
    // Fenced code blocks
    if (line.trimStart().startsWith("```")) {
      if (inCodeBlock) {
        htmlLines.push("</pre>");
        inCodeBlock = false;
      } else {
        if (inList) {
          htmlLines.push("</ul>");
          inList = false;
        }
        htmlLines.push("<pre>");
        inCodeBlock = true;
      }
      continue;
    }

    if (inCodeBlock) {
      htmlLines.push(escapeHtml(line));
      continue;
    }

    // Headings
    const headingMatch = line.match(/^(#{1,4})\s+(.+)$/);
    if (headingMatch) {
      if (inList) {
        htmlLines.push("</ul>");
        inList = false;
      }
      const level = Math.min(headingMatch[1].length + 2, 6); // offset by 2 since h1/h2 used by report
      htmlLines.push(`<h${level}>${inlineFormat(headingMatch[2])}</h${level}>`);
      continue;
    }

    // Unordered list items
    const listMatch = line.match(/^[\s]*[-*]\s+(.+)$/);
    if (listMatch) {
      if (!inList) {
        htmlLines.push("<ul>");
        inList = true;
      }
      htmlLines.push(`<li>${inlineFormat(listMatch[1])}</li>`);
      continue;
    }

    // Ordered list items
    const olMatch = line.match(/^[\s]*\d+\.\s+(.+)$/);
    if (olMatch) {
      if (!inList) {
        htmlLines.push("<ul>");
        inList = true;
      }
      htmlLines.push(`<li>${inlineFormat(olMatch[1])}</li>`);
      continue;
    }

    // Close list if we left it
    if (inList && line.trim() === "") {
      htmlLines.push("</ul>");
      inList = false;
    }

    // Regular paragraph
    if (line.trim()) {
      htmlLines.push(`<p>${inlineFormat(line)}</p>`);
    }
  }

  if (inList) htmlLines.push("</ul>");
  if (inCodeBlock) htmlLines.push("</pre>");

  return htmlLines.join("\n");
}

/** Apply inline formatting: bold, italic, inline code. */
function inlineFormat(text: string): string {
  let s = escapeHtml(text);
  s = s.replace(/\*\*(.+?)\*\*/g, "<strong>$1</strong>");
  s = s.replace(/\*(.+?)\*/g, "<em>$1</em>");
  s = s.replace(/`(.+?)`/g, "<code>$1</code>");
  return s;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

// ---------------------------------------------------------------------------
// Main entry point
// ---------------------------------------------------------------------------

/**
 * Generate a report from pipeline execution output.
 *
 * 1. Parses agent outputs for structured render blocks (chart/table data)
 * 2. Builds a JSON content summary
 * 3. Generates self-contained HTML for email delivery
 * 4. Stores everything in `sp_reports` for dashboard rendering
 */
export async function generateReport(
  sql: postgres.Sql,
  runId: string,
  pipelineName: string,
  agentOutputs: AgentOutput[],
): Promise<GeneratedReport> {
  // 1. Extract render blocks from agent outputs
  const renderBlocks = parseRenderBlocks(agentOutputs);

  // 2. Build structured content
  const contentJson = {
    pipeline: pipelineName,
    generatedAt: new Date().toISOString(),
    agentCount: agentOutputs.length,
    blockCount: renderBlocks.length,
    sections: agentOutputs.map((o) => ({
      agent: o.agentName,
      content: o.output,
    })),
  };

  // 3. Generate self-contained HTML
  const contentHtml = generateHtml(pipelineName, agentOutputs, renderBlocks);

  // 4. Store in database
  const title = `${pipelineName} Report — ${new Date().toLocaleDateString("en-US", { dateStyle: "medium" })}`;

  const [report] = await sql`
    INSERT INTO sp_reports (run_id, title, content_html, content_json, render_blocks)
    VALUES (
      ${runId},
      ${title},
      ${contentHtml},
      ${JSON.stringify(contentJson)},
      ${JSON.stringify(renderBlocks)}
    )
    RETURNING id
  `;

  return {
    id: report.id,
    title,
    contentHtml,
    contentJson,
    renderBlocks,
  };
}
