/**
 * Shared proxy helper for Ingestion API routes.
 * Forwards requests to the Rust backend at /api/ingestion/* endpoints.
 */

const API_BASE = process.env.API_BASE || "http://localhost:56415";

/** Proxy a request to the Rust backend and return the response. */
export async function ingestionProxy(
  backendPath: string,
  req: Request,
  { forwardBody = false }: { forwardBody?: boolean } = {},
): Promise<Response> {
  const url = new URL(req.url);
  const query = url.search;

  const headers: Record<string, string> = {};
  const ct = req.headers.get("Content-Type");
  if (ct) headers["Content-Type"] = ct;

  const init: RequestInit = {
    method: req.method,
    headers,
  };

  if (forwardBody && (req.method === "POST" || req.method === "PUT" || req.method === "PATCH")) {
    // For multipart uploads, forward the raw body stream
    if (ct?.includes("multipart/form-data")) {
      init.body = req.body;
      // @ts-expect-error — Node fetch supports duplex streaming
      init.duplex = "half";
    } else {
      init.body = await req.text();
    }
  }

  let res: Response;
  try {
    res = await fetch(`${API_BASE}${backendPath}${query}`, init);
  } catch (err) {
    const msg = err instanceof Error ? err.message : String(err);
    return new Response(
      JSON.stringify({ error: `Backend unreachable: ${msg}` }),
      { status: 502, headers: { "Content-Type": "application/json" } },
    );
  }

  return new Response(res.body, {
    status: res.status,
    headers: { "Content-Type": res.headers.get("Content-Type") || "application/json" },
  });
}
