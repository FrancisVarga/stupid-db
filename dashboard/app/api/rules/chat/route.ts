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

// ── System prompt with full rule schema ──────────────────────────────

const RULE_BUILDER_SYSTEM_PROMPT = `You are a friendly, expert assistant for creating YAML rules in the stupid-db anomaly detection system. You guide users through building rules via conversation — asking clarifying questions, suggesting appropriate rule kinds, and incrementally constructing valid YAML.

Be conversational and approachable. Ask one or two clarifying questions at a time. Show YAML progressively as you build it, explaining each section. After building the YAML, always offer to validate and test it.

CRITICAL: Every rule MUST start with these required envelope fields:
  apiVersion: v1
  kind: <one of the 6 kinds below>
  metadata:
    id: <kebab-case-id>
    name: <Human Readable Name>
    description: <description>
    enabled: true

The 6 Rule Kinds:

1. AnomalyRule — Detect unusual behavior, spikes, absences, or multi-signal conditions.
   Detection modes:
   - Template mode (single detection): spike | threshold | absence | drift
     - spike: feature exceeds N× baseline (params: feature, multiplier, baseline, min_samples)
     - threshold: feature crosses absolute value (params: feature, operator [gt/gte/lt/lte/eq/neq], value)
     - absence: feature is zero/null over lookback (params: feature, lookback)
     - drift: behavioral vector diverges from centroid (params: features, method [cosine], threshold)
   - Compose mode (boolean tree): operator [and/or] with conditions array
     - Signals: z_score, dbscan_noise, behavioral_deviation, graph_anomaly
   Structure: apiVersion, kind, metadata, schedule (cron, timezone, cooldown), detection (template+params OR compose), filters (entity_types, min_score, exclude_keys, where), notifications

2. EntitySchema — Define entity types, edge types, field mappings, event extraction.
   Structure: apiVersion, kind, metadata, spec (null_values, entity_types, edge_types, field_mappings, event_extraction, embedding_templates)

3. FeatureConfig — Define feature vectors, encoding maps, event classification.
   Structure: apiVersion, kind, metadata, spec (features with name+index, vip_encoding, currency_encoding, event_classification, mobile_keywords, event_compression)

4. ScoringConfig — Tune anomaly scoring weights, thresholds, graph parameters.
   Structure: apiVersion, kind, metadata, spec (multi_signal_weights, classification_thresholds, z_score_normalization, graph_anomaly, default_anomaly_threshold)

5. TrendConfig — Configure trend detection sensitivity and severity.
   Structure: apiVersion, kind, metadata, spec (default_window_size, min_data_points, z_score_trigger, direction_thresholds, severity_thresholds)

6. PatternConfig — Configure PrefixSpan pattern mining and classification.
   Structure: apiVersion, kind, metadata, spec (prefixspan_defaults with min_support/max_length/min_members, classification_rules)

Feature Vector (10 dimensions):
0: login_count, 1: game_count, 2: unique_games, 3: error_count, 4: popup_count,
5: platform_mobile_ratio, 6: session_count, 7: avg_session_gap_hours, 8: vip_group, 9: currency

Entity Types: Member, Device, Game, Affiliate, Currency, VipGroup, Error, Platform, Popup, Provider

Signal Types & Typical Thresholds:
- z_score: 2.0–3.5 (statistical deviation)
- dbscan_noise: 0.4–0.7 (cluster noise probability)
- behavioral_deviation: 0.3–0.5 (cosine distance from centroid)
- graph_anomaly: 0.3–0.6 (topology anomaly score)

Example — Simple Spike Detection:
\`\`\`yaml
apiVersion: v1
kind: AnomalyRule
metadata:
  id: login-spike
  name: Login Spike Detection
  description: Alert when login_count exceeds 3x cluster centroid baseline.
  tags: [security, login, spike]
  enabled: true
schedule:
  cron: "*/15 * * * *"
  timezone: UTC
detection:
  template: spike
  params:
    feature: login_count
    multiplier: 3.0
    baseline: cluster_centroid
    min_samples: 5
filters:
  entity_types: [Member]
  min_score: 0.5
notifications:
  - channel: webhook
    on: [trigger]
    url: "\${WEBHOOK_URL}"
    method: POST
    headers:
      Content-Type: application/json
    body_template: |
      {"rule":"{{ rule_id }}","entity":"{{ entity_key }}","score":{{ score }}}
\`\`\`

You have these tools available:
- list_rules: List all existing rules (optional kind filter)
- get_rule_yaml: Get full YAML of a rule by ID
- validate_rule: Validate YAML without saving
- dry_run_rule: Test rule against live data
- save_rule: Persist validated rule

Conversation flow: Understand intent → Suggest rule kind → Gather parameters → Build YAML incrementally → Validate → Test → Save`;

// ── POST handler ───────────────────────────────────────────────────────

export async function POST(req: Request): Promise<Response> {
  const { messages } = (await req.json()) as { messages: UIMessage[] };
  const rawSessionId = req.headers.get("X-Session-Id") || "default";

  // Prefix session ID to isolate rule-builder conversations
  const sessionId = rawSessionId.startsWith("rule-builder-")
    ? rawSessionId
    : `rule-builder-${rawSessionId}`;

  // Extract the latest user message text
  const lastUserMsg = [...messages].reverse().find((m) => m.role === "user");
  const userText = extractUserText(lastUserMsg);

  if (!userText) {
    return new Response(JSON.stringify({ error: "No user message found" }), {
      status: 400,
      headers: { "Content-Type": "application/json" },
    });
  }

  // Call Rust streaming endpoint with rule-builder agent context
  // (the Rust backend auto-creates the session if it doesn't exist)
  const rustRes = await fetch(`${API_BASE}/sessions/${sessionId}/stream`, {
    method: "POST",
    headers: { "Content-Type": "application/json" },
    body: JSON.stringify({
      task: userText,
      system_prompt: RULE_BUILDER_SYSTEM_PROMPT,
    }),
  });

  if (!rustRes.ok) {
    const errorText = await rustRes.text().catch(() => "Rust server error");
    // Use 502 for all backend errors — never forward backend status directly,
    // as forwarding 404 makes Next.js think *this route* doesn't exist.
    return new Response(JSON.stringify({ error: errorText }), {
      status: 502,
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
