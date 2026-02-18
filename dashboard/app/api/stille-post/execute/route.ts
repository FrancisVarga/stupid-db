export const dynamic = "force-dynamic";

const API_BASE = process.env.API_BASE || "http://localhost:3088";
const ANTHROPIC_API_KEY = process.env.ANTHROPIC_API_KEY || "";

// ── Types ──────────────────────────────────────────────────────────

interface ExecuteRequest {
  agent_id: string;
  input: string;
  pipeline_id?: string;
}

interface SpAgent {
  id: string;
  name: string;
  system_prompt: string;
  model: string;
}

// ── SSE helpers ────────────────────────────────────────────────────

function sseEvent(event: string, data: unknown): string {
  return `event: ${event}\ndata: ${JSON.stringify(data)}\n\n`;
}

// ── POST handler — SSE streaming agent execution ───────────────────

export async function POST(req: Request): Promise<Response> {
  let body: ExecuteRequest;
  try {
    body = await req.json();
  } catch {
    return new Response(JSON.stringify({ error: "Invalid JSON body" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  const { agent_id, input, pipeline_id } = body;
  if (!agent_id || !input) {
    return new Response(
      JSON.stringify({ error: "agent_id and input are required" }),
      { status: 400, headers: { "Content-Type": "application/json" } },
    );
  }

  if (!ANTHROPIC_API_KEY) {
    return new Response(
      JSON.stringify({ error: "ANTHROPIC_API_KEY not configured" }),
      { status: 500, headers: { "Content-Type": "application/json" } },
    );
  }

  // Load agent from Rust backend
  let agent: SpAgent;
  try {
    const res = await fetch(`${API_BASE}/sp/agents/${agent_id}`);
    if (!res.ok) {
      const text = await res.text();
      return new Response(
        JSON.stringify({ error: `Agent not found: ${text}` }),
        { status: res.status, headers: { "Content-Type": "application/json" } },
      );
    }
    agent = await res.json();
  } catch (err) {
    return new Response(
      JSON.stringify({ error: `Failed to load agent: ${err}` }),
      { status: 502, headers: { "Content-Type": "application/json" } },
    );
  }

  // Build an SSE ReadableStream
  const stream = new ReadableStream({
    async start(controller) {
      const encoder = new TextEncoder();
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(sseEvent(event, data)));
      };

      const startTime = Date.now();
      send("agent_start", {
        agent_id,
        agent_name: agent.name,
        pipeline_id: pipeline_id ?? null,
        timestamp: new Date().toISOString(),
      });

      try {
        // Call Anthropic streaming API
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
              model: agent.model || "claude-sonnet-4-6",
              max_tokens: 8192,
              stream: true,
              system: agent.system_prompt,
              messages: [{ role: "user", content: input }],
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

        // Parse Anthropic SSE stream
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
          // Keep the last incomplete line in the buffer
          buffer = lines.pop() ?? "";

          for (const line of lines) {
            if (!line.startsWith("data: ")) continue;
            const payload = line.slice(6).trim();
            if (payload === "[DONE]") continue;

            let event: {
              type?: string;
              delta?: { type?: string; text?: string };
              message?: { usage?: { input_tokens?: number; output_tokens?: number } };
              usage?: { output_tokens?: number };
              index?: number;
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

              case "content_block_stop":
                send("agent_step", {
                  step: "content_block_complete",
                  index: event.index ?? 0,
                  timestamp: new Date().toISOString(),
                });
                break;
            }
          }
        }

        send("agent_complete", {
          output: fullOutput,
          tokens_used: inputTokens + outputTokens,
          input_tokens: inputTokens,
          output_tokens: outputTokens,
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
