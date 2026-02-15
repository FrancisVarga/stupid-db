// ── Athena SSE query client — streams results from POST endpoint ────

const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:56415";

// ── Types ──────────────────────────────────────────────────────────

export interface AthenaQueryCallbacks {
  onStatus?: (state: string, queryId: string, message: string) => void;
  onColumns?: (columns: string[]) => void;
  onRows?: (rows: string[][]) => void;
  onDone?: (totalRows: number, queryId: string) => void;
  onError?: (message: string) => void;
}

// ── SSE streaming query execution ──────────────────────────────────

/**
 * Execute a SQL query against an Athena connection via SSE.
 *
 * Uses fetch + ReadableStream to parse SSE from a POST request
 * (EventSource only supports GET, so manual SSE parsing is required).
 *
 * Returns an AbortController so the caller can cancel the query.
 */
export function executeAthenaQuery(
  connectionId: string,
  sql: string,
  database: string | undefined,
  callbacks: AthenaQueryCallbacks,
): AbortController {
  const controller = new AbortController();

  fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(connectionId)}/query`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sql, database }),
      signal: controller.signal,
    },
  )
    .then(async (response) => {
      if (!response.ok) {
        callbacks.onError?.(await response.text());
        return;
      }

      const reader = response.body!.getReader();
      const decoder = new TextDecoder();
      let buffer = "";

      while (true) {
        const { done, value } = await reader.read();
        if (done) break;

        buffer += decoder.decode(value, { stream: true });

        // Parse SSE format: "event: type\ndata: json\n\n"
        const parts = buffer.split("\n\n");
        buffer = parts.pop() || "";

        for (const part of parts) {
          let eventType = "message";
          let data = "";

          for (const line of part.split("\n")) {
            if (line.startsWith("event: ")) {
              eventType = line.slice(7).trim();
            } else if (line.startsWith("data: ")) {
              data = line.slice(6);
            }
          }

          if (!data) continue;

          try {
            const parsed = JSON.parse(data);

            switch (eventType) {
              case "status":
                callbacks.onStatus?.(
                  parsed.state,
                  parsed.query_id,
                  parsed.message || "",
                );
                break;
              case "columns":
                callbacks.onColumns?.(parsed);
                break;
              case "rows":
                callbacks.onRows?.(parsed);
                break;
              case "done":
                callbacks.onDone?.(parsed.total_rows, parsed.query_id);
                break;
              case "error":
                callbacks.onError?.(parsed.message);
                break;
            }
          } catch {
            // Malformed SSE data — skip silently
          }
        }
      }
    })
    .catch((err: Error) => {
      if (err.name !== "AbortError") {
        callbacks.onError?.(err.message);
      }
    });

  return controller;
}
