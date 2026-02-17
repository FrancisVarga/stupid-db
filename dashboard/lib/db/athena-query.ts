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
              case "columns": {
                // Backend sends {"columns": [{name, type}, ...]} — extract names
                const cols = parsed.columns ?? parsed;
                const names = Array.isArray(cols)
                  ? cols.map((c: string | { name: string }) =>
                      typeof c === "string" ? c : c.name,
                    )
                  : cols;
                callbacks.onColumns?.(names);
                break;
              }
              case "rows":
                callbacks.onRows?.(parsed.rows ?? parsed);
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

// ── Parquet export ──────────────────────────────────────────────────

/**
 * Execute an Athena query and download the result as a Parquet file.
 *
 * The backend runs the query, converts to typed Parquet (Zstd compressed),
 * persists a copy to data/exports/athena/, and streams the file back.
 */
export async function downloadAthenaParquet(
  connectionId: string,
  sql: string,
  database: string | undefined,
): Promise<void> {
  const response = await fetch(
    `${API_BASE}/athena-connections/${encodeURIComponent(connectionId)}/query/parquet`,
    {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ sql, database }),
    },
  );

  if (!response.ok) {
    const text = await response.text();
    throw new Error(text || `HTTP ${response.status}`);
  }

  // Extract filename from Content-Disposition header or generate one.
  const disposition = response.headers.get("Content-Disposition") || "";
  const match = disposition.match(/filename="?([^"]+)"?/);
  const filename = match?.[1] || `athena-export-${Date.now()}.parquet`;

  // Trigger browser download.
  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const a = document.createElement("a");
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}
