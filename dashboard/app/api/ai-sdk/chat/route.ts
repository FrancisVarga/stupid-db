import { anthropic } from "@ai-sdk/anthropic";
import { claudeCode } from "ai-sdk-provider-claude-code";
import {
  convertToModelMessages,
  streamText,
  type UIMessage,
  type LanguageModelUsage,
} from "ai";

export const maxDuration = 60;

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

const SYSTEM_PROMPT = `You are a helpful AI assistant integrated into the stupid-db dashboard.
You have expertise in data analysis, SQL queries, anomaly detection, and graph databases.
When answering questions about data patterns, entities, or anomalies, be precise and reference specific metrics.
Format code blocks with the appropriate language tag.
Be concise but thorough.`;

// Metadata type for token tracking on the client
export type ChatMetadata = {
  model?: string;
  createdAt?: number;
  totalUsage?: LanguageModelUsage;
};

export type ChatUIMessage = UIMessage<ChatMetadata>;

export async function POST(req: Request): Promise<Response> {
  const {
    messages,
    model: requestedModel,
    provider: requestedProvider,
  } = (await req.json()) as {
    messages: UIMessage[];
    model?: string;
    provider?: ProviderType;
  };

  // Determine provider
  const provider: ProviderType = requestedProvider ?? DEFAULT_PROVIDER;

  // Select model and provider instance
  let modelInstance: ReturnType<typeof anthropic> | ReturnType<typeof claudeCode>;
  let modelLabel: string;

  if (provider === "claude-code") {
    // Claude Code provider
    const ccModelId =
      requestedModel && requestedModel in CLAUDE_CODE_MODELS
        ? CLAUDE_CODE_MODELS[requestedModel]
        : "sonnet";
    modelInstance = claudeCode(ccModelId);
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

  const result = streamText({
    model: modelInstance,
    system: SYSTEM_PROMPT,
    messages: await convertToModelMessages(messages),
  });

  return result.toUIMessageStreamResponse({
    sendReasoning: true,
    messageMetadata: ({ part }) => {
      if (part.type === "start") {
        return {
          model: modelLabel,
          createdAt: Date.now(),
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
