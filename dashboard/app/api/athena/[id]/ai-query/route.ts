// ── Athena AI Query route — schema-aware SQL generation via streaming ──

import { anthropic } from "@ai-sdk/anthropic";
import {
  convertToModelMessages,
  stepCountIs,
  streamText,
  type UIMessage,
} from "ai";
import { formatSchemaForPrompt } from "@/lib/ai-sdk/tools/athena-schema";
import type { AthenaSchema } from "@/lib/db/athena-connections";

export const maxDuration = 60;

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

const MODEL = "claude-sonnet-4-6";

/** Fetch schema from the Rust backend (server-side only). */
async function fetchSchema(
  connectionId: string,
): Promise<AthenaSchema | null> {
  try {
    const res = await fetch(
      `${API_BASE}/athena-connections/${encodeURIComponent(connectionId)}/schema`,
      { cache: "no-store" },
    );
    if (!res.ok) return null;
    const data = (await res.json()) as {
      schema_status: string;
      schema: AthenaSchema | null;
    };
    return data.schema;
  } catch {
    return null;
  }
}

/** Build system prompt with Athena schema context. */
function buildSystemPrompt(schemaText: string, database?: string): string {
  const dbHint = database ? `\nThe default database is "${database}".` : "";

  return `You are an Athena SQL assistant. You help users write and debug AWS Athena (Presto/Trino) SQL queries.

## Your Capabilities
- Generate valid Athena SQL from natural language requests
- Explain query results and suggest follow-ups
- Debug and fix failed queries when given error messages
- Suggest optimizations (partitions, LIMIT clauses, column pruning)

## Rules
- ALWAYS output SQL inside a fenced code block with the \`sql\` language tag
- ALWAYS add LIMIT 100 unless the user specifies a different limit
- Use Athena/Presto SQL syntax (not MySQL or PostgreSQL)
- Reference only tables and columns that exist in the schema below
- When a query fails, analyze the error and generate corrected SQL
- Be concise — explain briefly, then show the SQL${dbHint}

## Available Schema
${schemaText}`;
}

export async function POST(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id: connectionId } = await params;

  const { messages, database } = (await req.json()) as {
    messages: UIMessage[];
    database?: string;
  };

  // Fetch schema server-side
  const schema = await fetchSchema(connectionId);
  const schemaText = formatSchemaForPrompt(schema);

  const result = streamText({
    model: anthropic(MODEL),
    system: buildSystemPrompt(schemaText, database),
    messages: await convertToModelMessages(messages),
    stopWhen: stepCountIs(5),
  });

  return result.toUIMessageStreamResponse({
    sendReasoning: true,
  });
}
