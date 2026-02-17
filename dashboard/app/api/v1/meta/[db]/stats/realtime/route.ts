import { NextRequest } from "next/server";
import { getPool } from "@/lib/db/client";
import { getRealtimeStats } from "@/lib/db/introspect";

export const dynamic = "force-dynamic";

const INTERVAL_MS = 2000;

/**
 * SSE endpoint: streams realtime PG stats every 2 seconds.
 * Automatically closes when the client disconnects.
 */
export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;

  let sql: Awaited<ReturnType<typeof getPool>>;
  try {
    sql = await getPool(db);
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return new Response(JSON.stringify({ error: message }), {
      status: 500,
      headers: { "Content-Type": "application/json" },
    });
  }

  const encoder = new TextEncoder();
  const abortSignal = req.signal;

  const stream = new ReadableStream({
    async start(controller) {
      const enqueue = (data: string) => {
        try {
          controller.enqueue(encoder.encode(`data: ${data}\n\n`));
        } catch {
          // stream closed
        }
      };

      const tick = async () => {
        if (abortSignal.aborted) return;
        try {
          const stats = await getRealtimeStats(sql);
          enqueue(JSON.stringify(stats));
        } catch (err) {
          const msg = err instanceof Error ? err.message : String(err);
          console.error("[realtime-stats]", msg);
          enqueue(JSON.stringify({ error: msg }));
        }
      };

      // Send first sample immediately
      await tick();

      const interval = setInterval(tick, INTERVAL_MS);

      // Clean up when client disconnects
      abortSignal.addEventListener("abort", () => {
        clearInterval(interval);
        try {
          controller.close();
        } catch {
          // already closed
        }
      });
    },
  });

  return new Response(stream, {
    headers: {
      "Content-Type": "text/event-stream",
      "Cache-Control": "no-cache, no-transform",
      Connection: "keep-alive",
    },
  });
}
