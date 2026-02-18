import { anthropic } from "@ai-sdk/anthropic";
import { createClaudeCode } from "ai-sdk-provider-claude-code";
import path from "path";
import {
  convertToModelMessages,
  stepCountIs,
  streamText,
  type UIMessage,
  type LanguageModelUsage,
} from "ai";
import sql from "@/lib/db/ai-sdk";
import { ensureAiSdkTables } from "@/lib/db/ai-sdk-migrate";
import { dbQueryTool } from "@/lib/ai-sdk/tools/db-query";
import { memoryTool } from "@/lib/ai-sdk/tools/memory";

export const maxDuration = 60;

const migrated = ensureAiSdkTables();

// Provider types
type ProviderType = "anthropic" | "claude-code";

// Models allowed via the `model` body field (anthropic provider)
const ALLOWED_MODELS: Record<string, string> = {
  "claude-sonnet-4-6": "claude-sonnet-4-6",
  "claude-opus-4-6": "claude-opus-4-6",
  "claude-sonnet-4-5": "claude-sonnet-4-5",
  "claude-opus-4-5": "claude-opus-4-5",
  "claude-haiku-4-5": "claude-haiku-4-5",
};

// Claude Code provider models (shortcuts)
const CLAUDE_CODE_MODELS: Record<string, string> = {
  "cc-opus": "opus",
  "cc-sonnet": "sonnet",
  "cc-haiku": "haiku",
};

const DEFAULT_MODEL = "claude-sonnet-4-6";
const DEFAULT_PROVIDER: ProviderType = "anthropic";

const BASE_SYSTEM_PROMPT = `You are a helpful AI assistant integrated into the stupid-db dashboard.
You have expertise in data analysis, SQL queries, anomaly detection, and graph databases.
When answering questions about data patterns, entities, or anomalies, be precise and reference specific metrics.
Format code blocks with the appropriate language tag.
Be concise but thorough.`;

const MEMORY_LIMIT = 5;

/** Search memories relevant to the user's latest message via full-text search. */
async function searchMemories(
  query: string,
): Promise<Array<{ content: string; category: string | null }>> {
  const terms = query.split(/\s+/).filter(Boolean);
  if (terms.length === 0) return [];

  const tsquery = terms.join(" & ");
  try {
    return await sql`
      SELECT content, category
      FROM memories
      WHERE to_tsvector('english', content) @@ to_tsquery('english', ${tsquery})
      ORDER BY ts_rank(to_tsvector('english', content), to_tsquery('english', ${tsquery})) DESC,
               updated_at DESC
      LIMIT ${MEMORY_LIMIT}
    `;
  } catch {
    // If FTS fails (e.g. bad tsquery syntax), fall back silently
    return [];
  }
}

/** Build system prompt, optionally enriched with relevant memories. */
function buildSystemPrompt(
  memories: Array<{ content: string; category: string | null }>,
): string {
  if (memories.length === 0) return BASE_SYSTEM_PROMPT;

  const memoryBlock = memories
    .map((m) => {
      const prefix = m.category ? `[${m.category}] ` : "";
      return `- ${prefix}${m.content}`;
    })
    .join("\n");

  return `${BASE_SYSTEM_PROMPT}

<user_memories>
The following are relevant memories/notes saved by the user. Reference them when applicable:
${memoryBlock}
</user_memories>`;
}

// Metadata type for token tracking on the client
export type ChatMetadata = {
  model?: string;
  createdAt?: number;
  totalUsage?: LanguageModelUsage;
  sessionId?: string;
};

export type ChatUIMessage = UIMessage<ChatMetadata>;

export async function POST(req: Request): Promise<Response> {
  await migrated;

  const {
    messages,
    model: requestedModel,
    provider: requestedProvider,
    sessionId: requestedSessionId,
  } = (await req.json()) as {
    messages: UIMessage[];
    model?: string;
    provider?: ProviderType;
    sessionId?: string;
  };

  // Determine provider
  const provider: ProviderType = requestedProvider ?? DEFAULT_PROVIDER;

  // Select model and provider instance
  let modelInstance: ReturnType<typeof anthropic> | ReturnType<ReturnType<typeof createClaudeCode>>;
  let modelLabel: string;

  if (provider === "claude-code") {
    // Claude Code provider with custom project root
    const ccModelId =
      requestedModel && requestedModel in CLAUDE_CODE_MODELS
        ? CLAUDE_CODE_MODELS[requestedModel]
        : "sonnet";

    // Set project root to agents/stupid-db-claude-code
    const projectRoot = path.resolve(process.cwd(), "..", "agents", "stupid-db-claude-code");
    const claudeCodeProvider = createClaudeCode({
      defaultSettings: {
        cwd: projectRoot,
      },
    });

    modelInstance = claudeCodeProvider(ccModelId);
    modelLabel = `claude-code:${ccModelId}`;
  } else {
    // Anthropic provider (default)
    const anthropicModelId =
      requestedModel && requestedModel in ALLOWED_MODELS
        ? ALLOWED_MODELS[requestedModel]
        : DEFAULT_MODEL;
    modelInstance = anthropic(anthropicModelId);
    modelLabel = anthropicModelId;
  }

  // Resolve or create session for persistence
  let sessionId = requestedSessionId;
  if (!sessionId) {
    const [session] = await sql`
      INSERT INTO chat_sessions (provider, model)
      VALUES (${provider}, ${modelLabel})
      RETURNING id
    `;
    sessionId = session.id;
  } else {
    // Touch updated_at on existing session
    await sql`
      UPDATE chat_sessions SET updated_at = now() WHERE id = ${sessionId}
    `;
  }

  // Persist the latest user message (last message in the array)
  const lastMessage = messages[messages.length - 1];
  if (lastMessage && lastMessage.role === "user") {
    await sql`
      INSERT INTO chat_messages (session_id, role, content, metadata)
      VALUES (
        ${sessionId!},
        ${lastMessage.role},
        ${JSON.stringify(lastMessage.parts)},
        ${JSON.stringify({ id: lastMessage.id })}
      )
    `;
  }

  // Search memories relevant to the latest user message
  const userText = lastMessage?.role === "user"
    ? lastMessage.parts
        .filter((p): p is { type: "text"; text: string } => p.type === "text")
        .map((p) => p.text)
        .join(" ")
    : "";
  const memories = userText ? await searchMemories(userText) : [];

  const result = streamText({
    model: modelInstance,
    system: buildSystemPrompt(memories),
    messages: await convertToModelMessages(messages),
    tools: { db_query: dbQueryTool, memory: memoryTool },
    stopWhen: stepCountIs(5),
    onFinish: async ({ text, toolCalls, usage, finishReason }) => {
      // Persist the assistant response after streaming completes
      await sql`
        INSERT INTO chat_messages (session_id, role, content, metadata)
        VALUES (
          ${sessionId!},
          'assistant',
          ${JSON.stringify([{ type: "text", text }])},
          ${JSON.stringify({
            toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
            usage,
            finishReason,
            model: modelLabel,
          })}
        )
      `;
    },
  });

  return result.toUIMessageStreamResponse({
    sendReasoning: true,
    messageMetadata: ({ part }) => {
      if (part.type === "start") {
        return {
          model: modelLabel,
          createdAt: Date.now(),
          sessionId,
        };
      }
      if (part.type === "finish") {
        return {
          totalUsage: part.totalUsage,
        };
      }
    },
  });
}
