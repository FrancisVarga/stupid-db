import {
  createUIMessageStream,
  createUIMessageStreamResponse,
} from "ai";

export const dynamic = "force-dynamic";

const API_BASE = process.env.API_BASE || "http://localhost:3088";

// ── Rust StreamEvent types ─────────────────────────────────────────────

type RustStreamEvent =
  | { TextDelta: { text: string } }
  | { ToolCallStart: { id: string; name: string } }
  | { ToolCallDelta: { id: string; arguments_delta: string } }
  | { ToolCallEnd: { id: string } }
  | { ToolExecutionStart: { id: string; name: string } }
  | { ToolExecutionResult: { id: string; content: string; is_error: boolean } }
  | { MessageEnd: { stop_reason: "EndTurn" | "ToolUse" | "MaxTokens" | "StopSequence" } }
  | { Error: { message: string } };

// ── AI SDK UIMessage types (from useChat) ──────────────────────────────

interface UIMessage {
  role: string;
  content: string;
  parts?: Array<{ type: string; text?: string }>;
}

// ── POST handler ───────────────────────────────────────────────────────

export async function POST(req: Request): Promise<Response> {
  const { messages } = (await req.json()) as { messages: UIMessage[] };
  const sessionId = req.headers.get("X-Session-Id") || "default";

  // Extract the latest user message text
  const lastUserMsg = [...messages].reverse().find((m) => m.role === "user");
  const userText = extractUserText(lastUserMsg);

  if (!userText) {
    return new Response(JSON.stringify({ error: "No user message found" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Call Rust streaming endpoint
  const rustRes = await fetch(`${API_BASE}/sessions/${sessionId}/stream`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({ task: userText }),
  });

  if (!rustRes.ok) {
    const errorText = await rustRes.text().catch(() => "Rust server error");
    return new Response(JSON.stringify({ error: errorText }), {
      status: rustRes.status,
      headers: { "Content-Type": "application/json" },
    });
  }

  if (!rustRes.body) {
    return new Response(JSON.stringify({ error: "No response body" }), {
      status: 502,
      headers: { "Content-Type": "application/json" },
    });
  }

  const rustBody = rustRes.body;

  return createUIMessageStreamResponse({
    stream: createUIMessageStream({
      execute: async ({ writer }) => {
        await translateRustSSE(rustBody, writer);
      },
    }),
  });
}

// ── SSE parser + translator ────────────────────────────────────────────

type UIWriter = Parameters<
  Parameters<typeof createUIMessageStream>[0]["execute"]
>[0]["writer"];

async function translateRustSSE(
  body: ReadableStream<Uint8Array>,
  writer: UIWriter,
): Promise<void> {
  const reader = body.getReader();
  const decoder = new TextDecoder();

  // Mutable state for buffering
  let sseBuffer = "";
  let textPartId: string | null = null;
  let textIdCounter = 0;
  const toolCallBuffers = new Map<
    string,
    { name: string; argChunks: string[] }
  >();

  // Emit message start
  writer.write({ type: "start" });
  writer.write({ type: "start-step" });

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;

      sseBuffer += decoder.decode(value, { stream: true });

      // Split on double newline (SSE event boundary)
      const parts = sseBuffer.split("\n\n");
      // Keep the last incomplete chunk in the buffer
      sseBuffer = parts.pop() ?? "";

      for (const part of parts) {
        const dataLine = part
          .split("\n")
          .find((line) => line.startsWith("data: "));
        if (!dataLine) continue;

        const jsonStr = dataLine.slice(6); // strip "data: "
        let event: RustStreamEvent;
        try {
          event = JSON.parse(jsonStr);
        } catch {
          continue; // skip malformed events
        }

        processEvent(event, writer, {
          getTextPartId: () => textPartId,
          setTextPartId: (id) => { textPartId = id; },
          nextTextId: () => `text-${textIdCounter++}`,
          toolCallBuffers,
        });
      }
    }

    // End any open text part
    if (textPartId) {
      writer.write({ type: "text-end", id: textPartId });
    }

    writer.write({ type: "finish-step" });
    writer.write({ type: "finish", finishReason: "stop" });
  } finally {
    reader.releaseLock();
  }
}

// ── Event processing ───────────────────────────────────────────────────

interface TranslationState {
  getTextPartId: () => string | null;
  setTextPartId: (id: string | null) => void;
  nextTextId: () => string;
  toolCallBuffers: Map<string, { name: string; argChunks: string[] }>;
}

function processEvent(
  event: RustStreamEvent,
  writer: UIWriter,
  state: TranslationState,
): void {
  if ("TextDelta" in event) {
    // Start a text part if not open
    if (!state.getTextPartId()) {
      const id = state.nextTextId();
      state.setTextPartId(id);
      writer.write({ type: "text-start", id });
    }
    writer.write({
      type: "text-delta",
      id: state.getTextPartId()!,
      delta: event.TextDelta.text,
    });
    return;
  }

  if ("ToolCallStart" in event) {
    // Close any open text part before tool call
    closeTextPart(writer, state);

    const { id, name } = event.ToolCallStart;
    state.toolCallBuffers.set(id, { name, argChunks: [] });
    writer.write({
      type: "tool-input-start",
      toolCallId: id,
      toolName: name,
    });
    return;
  }

  if ("ToolCallDelta" in event) {
    const { id, arguments_delta } = event.ToolCallDelta;
    const buf = state.toolCallBuffers.get(id);
    if (buf) {
      buf.argChunks.push(arguments_delta);
    }
    writer.write({
      type: "tool-input-delta",
      toolCallId: id,
      inputTextDelta: arguments_delta,
    });
    return;
  }

  if ("ToolCallEnd" in event) {
    const { id } = event.ToolCallEnd;
    const buf = state.toolCallBuffers.get(id);
    if (buf) {
      const argsJson = buf.argChunks.join("");
      let input: unknown;
      try {
        input = JSON.parse(argsJson);
      } catch {
        input = argsJson;
      }
      writer.write({
        type: "tool-input-available",
        toolCallId: id,
        toolName: buf.name,
        input,
      });
    }
    return;
  }

  if ("ToolExecutionStart" in event) {
    // Emit as a custom data part for UI feedback
    writer.write({
      type: "data-tool-execution",
      data: {
        toolCallId: event.ToolExecutionStart.id,
        toolName: event.ToolExecutionStart.name,
        status: "running",
      },
    });
    return;
  }

  if ("ToolExecutionResult" in event) {
    const { id, content, is_error } = event.ToolExecutionResult;
    if (is_error) {
      writer.write({
        type: "tool-output-error",
        toolCallId: id,
        errorText: content,
      });
    } else {
      let output: unknown;
      try {
        output = JSON.parse(content);
      } catch {
        output = content;
      }
      writer.write({
        type: "tool-output-available",
        toolCallId: id,
        output,
      });
    }
    // Clean up buffer
    state.toolCallBuffers.delete(id);

    // After tool results, a new step begins
    writer.write({ type: "finish-step" });
    writer.write({ type: "start-step" });
    return;
  }

  if ("MessageEnd" in event) {
    closeTextPart(writer, state);
    const { stop_reason } = event.MessageEnd;
    if (stop_reason === "ToolUse") {
      // Step boundary — the runtime will continue with tool results
      writer.write({ type: "finish-step" });
      writer.write({ type: "start-step" });
    }
    // EndTurn/MaxTokens/StopSequence: handled by the outer finish
    return;
  }

  if ("Error" in event) {
    closeTextPart(writer, state);
    writer.write({ type: "error", errorText: event.Error.message });
    return;
  }
}

// ── Helpers ────────────────────────────────────────────────────────────

function closeTextPart(writer: UIWriter, state: TranslationState): void {
  const id = state.getTextPartId();
  if (id) {
    writer.write({ type: "text-end", id });
    state.setTextPartId(null);
  }
}

function extractUserText(msg: UIMessage | undefined): string | null {
  if (!msg) return null;

  // AI SDK v6 stores text in parts array
  if (msg.parts) {
    const textPart = msg.parts.find((p) => p.type === "text" && p.text);
    if (textPart?.text) return textPart.text;
  }

  // Fallback to content string
  if (msg.content) return msg.content;

  return null;
}
