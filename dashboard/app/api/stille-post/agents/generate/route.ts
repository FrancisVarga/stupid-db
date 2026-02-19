export const dynamic = "force-dynamic";

const ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY || "";

const AGENT_CREATOR_SYSTEM_PROMPT = `You are an expert AI agent designer for the Stille Post platform — an AI agent orchestration system for data analytics and reporting.

Your job is to help users create Stille Post agents by generating complete agent configurations based on their natural language descriptions.

## What You Generate

When the user describes what kind of agent they want, respond with EXACTLY this JSON structure inside a \`\`\`json code block:

\`\`\`json
{
  "name": "kebab-case-name",
  "description": "One sentence describing the agent's purpose",
  "system_prompt": "The full system prompt for the agent...",
  "model": "claude-sonnet-4-6",
  "template_id": null,
  "skills_config": [],
  "mcp_servers_config": [],
  "tools_config": []
}
\`\`\`

## Guidelines

**Name**: Use kebab-case, descriptive, concise (e.g., "fraud-detector", "daily-kpi-reporter")

**Model choices**:
- \`claude-sonnet-4-6\` — Best balance of speed and quality (default)
- \`claude-opus-4-6\` — Most capable, use for complex multi-step analysis
- \`claude-haiku-4-5\` — Fastest, use for simple classification/extraction

**System prompt best practices**:
- Start with a clear role definition ("You are a...")
- Specify the domain context (online gaming platform, data analytics, etc.)
- Define the expected output format (structured JSON, markdown report, etc.)
- Include specific fields the output should contain
- Add constraints and quality requirements

**Template IDs** (optional, set if the agent fits a known archetype):
- \`security-analyst\` — Security event analysis
- \`trend-detective\` — Trend detection and behavioral shifts
- \`performance-monitor\` — System performance monitoring
- \`executive-summarizer\` — Executive-level summaries
- \`data-quality-auditor\` — Data quality and integrity checks

**tools_config**: Array of tool objects the agent can use. Common tools:
- \`{"name": "athena-query", "description": "Query the Athena data warehouse"}\`
- \`{"name": "entity-lookup", "description": "Look up entity details by key"}\`
- \`{"name": "anomaly-check", "description": "Check recent anomaly detections"}\`

## Conversation Flow

1. User describes what they want → Generate the full JSON config
2. User asks for changes → Output the FULL updated JSON (not a diff)
3. Always include the \`\`\`json block so the UI can parse it
4. Add a brief explanation before or after the JSON block
5. If the request is vague, ask one clarifying question, then generate your best guess

Keep responses concise. Focus on generating great system prompts that produce structured, actionable output.`;

interface GenerateRequest {
  messages: { role: "user" | "assistant"; content: string }[];
}

function sseEvent(event: string, data: unknown): string {
  return `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
}

export async function POST(req: Request): Promise<Response> {
  let body: GenerateRequest;
  try {
    body = await req.json();
  } catch {
    return new Response(JSON.stringify({ error: "Invalid JSON body" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  if (!body.messages?.length) {
    return new Response(
      JSON.stringify({ error: "messages array is required" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  if (!ANTHROPIC_API_KEY) {
    return new Response(
      JSON.stringify({ error: "ANTHROPIC_API_KEY not configured" }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }

  const stream = new ReadableStream({
    async start(controller) {
      const encoder = new TextEncoder();
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(sseEvent(event, data)));
      };

      const startTime = Date.now();
      send("agent_start", {
        agent_name: "agent-creator",
        timestamp: new Date().toISOString(),
      });

      try {
        const anthropicRes = await fetch(
          "https://api.anthropic.com/v1/messages",
          {
            method: "POST",
            headers: {
              "Content-Type": "application/json",
              "x-api-key": ANTHROPIC_API_KEY,
              "anthropic-version": "2023-06-01",
            },
            body: JSON.stringify({
              model: "claude-sonnet-4-6",
              max_tokens: 4096,
              stream: true,
              system: AGENT_CREATOR_SYSTEM_PROMPT,
              messages: body.messages,
            }),
          },
        );

        if (!anthropicRes.ok) {
          const errText = await anthropicRes.text();
          send("agent_error", {
            error: `Anthropic API error: ${anthropicRes.status} ${errText}`,
            timestamp: new Date().toISOString(),
          });
          controller.close();
          return;
        }

        const reader = anthropicRes.body?.getReader();
        if (!reader) {
          send("agent_error", {
            error: "No response body from Anthropic API",
            timestamp: new Date().toISOString(),
          });
          controller.close();
          return;
        }

        const decoder = new TextDecoder();
        let buffer = "";
        let fullOutput = "";
        let inputTokens = 0;
        let outputTokens = 0;

        while (true) {
          const { done, value } = await reader.read();
          if (done) break;

          buffer += decoder.decode(value, { stream: true });
          const lines = buffer.split("\n");
          buffer = lines.pop() ?? "";

          for (const line of lines) {
            if (!line.startsWith("data: ")) continue;
            const payload = line.slice(6).trim();
            if (payload === "[DONE]") continue;

            let event: {
              type?: string;
              delta?: { type?: string; text?: string };
              message?: {
                usage?: { input_tokens?: number; output_tokens?: number };
              };
              usage?: { output_tokens?: number };
            };
            try {
              event = JSON.parse(payload);
            } catch {
              continue;
            }

            switch (event.type) {
              case "message_start":
                if (event.message?.usage?.input_tokens) {
                  inputTokens = event.message.usage.input_tokens;
                }
                break;
              case "content_block_delta":
                if (event.delta?.type === "text_delta" && event.delta.text) {
                  fullOutput += event.delta.text;
                  send("agent_token", { token: event.delta.text });
                }
                break;
              case "message_delta":
                if (event.usage?.output_tokens) {
                  outputTokens = event.usage.output_tokens;
                }
                break;
            }
          }
        }

        send("agent_complete", {
          output: fullOutput,
          tokens_used: inputTokens + outputTokens,
          duration_ms: Date.now() - startTime,
          timestamp: new Date().toISOString(),
        });
      } catch (err) {
        send("agent_error", {
          error: `Execution failed: ${err instanceof Error ? err.message : String(err)}`,
          timestamp: new Date().toISOString(),
        });
      }

      controller.close();
    },
  });

  return new Response(stream, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache",
      Connection: "keep-alive",
    },
  });
}
