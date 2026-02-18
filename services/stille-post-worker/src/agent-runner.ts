import type { Config } from "./config.js";
import type { SpAgent } from "./types.js";
import { getDb } from "./db.js";

export interface AgentExecutionResult {
  output: string;
  tokensUsed: number;
  durationMs: number;
  error?: string;
}

/**
 * Load agent config from database and execute it with the given input.
 * Uses Anthropic Messages API for execution.
 *
 * TODO: Migrate to @anthropic-ai/claude-code Agent SDK when stable.
 */
export async function runAgent(
  agentId: string,
  input: string,
  config: Config,
): Promise<AgentExecutionResult> {
  const start = Date.now();
  const sql = getDb(config);

  // Load agent from DB
  const agents = await sql`SELECT * FROM sp_agents WHERE id = ${agentId}`;
  if (agents.length === 0) {
    throw new Error(`Agent not found: ${agentId}`);
  }
  const agent = agents[0] as SpAgent;

  const response = await fetch("https://api.anthropic.com/v1/messages", {
    method: "POST",
    headers: {
      "Content-Type": "application/json",
      "x-api-key": config.anthropicApiKey,
      "anthropic-version": "2023-06-01",
    },
    body: JSON.stringify({
      model: agent.model || "claude-sonnet-4-6",
      max_tokens: 8192,
      system: agent.system_prompt,
      messages: [{ role: "user", content: input }],
    }),
  });

  if (!response.ok) {
    const err = await response.text();
    return {
      output: "",
      tokensUsed: 0,
      durationMs: Date.now() - start,
      error: `API error: ${response.status} ${err}`,
    };
  }

  const result = (await response.json()) as {
    content?: Array<{ type: string; text?: string }>;
    usage?: { input_tokens?: number; output_tokens?: number };
  };
  const textContent = result.content?.find((c) => c.type === "text");

  return {
    output: textContent?.text || "",
    tokensUsed:
      (result.usage?.input_tokens || 0) +
      (result.usage?.output_tokens || 0),
    durationMs: Date.now() - start,
  };
}

/**
 * Run agent by ID with structured input/output.
 * Serializes object input to JSON before passing to the agent.
 */
export async function executeAgentStep(
  agentId: string,
  input: Record<string, unknown>,
  config: Config,
): Promise<AgentExecutionResult> {
  const inputStr =
    typeof input === "string" ? input : JSON.stringify(input, null, 2);
  return runAgent(agentId, inputStr, config);
}
