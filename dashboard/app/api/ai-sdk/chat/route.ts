import { anthropic } from "@ai-sdk/anthropic";
import {
  convertToModelMessages,
  streamText,
  type UIMessage,
  type LanguageModelUsage,
} from "ai";

export const maxDuration = 60;

// Models allowed via the `model` body field
const ALLOWED_MODELS: Record<string, string> = {
  "claude-sonnet-4-6": "claude-sonnet-4-6",
  "claude-opus-4-6": "claude-opus-4-6",
  "claude-sonnet-4-5": "claude-sonnet-4-5",
  "claude-opus-4-5": "claude-opus-4-5",
  "claude-haiku-4-5": "claude-haiku-4-5",
};

const DEFAULT_MODEL = "claude-sonnet-4-6";

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
  const { messages, model: requestedModel } = (await req.json()) as {
    messages: UIMessage[];
    model?: string;
  };

  const modelId =
    requestedModel && requestedModel in ALLOWED_MODELS
      ? ALLOWED_MODELS[requestedModel]
      : DEFAULT_MODEL;

  const result = streamText({
    model: anthropic(modelId),
    system: SYSTEM_PROMPT,
    messages: await convertToModelMessages(messages),
  });

  return result.toUIMessageStreamResponse({
    sendReasoning: true,
    messageMetadata: ({ part }) => {
      if (part.type === "start") {
        return {
          model: modelId,
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
