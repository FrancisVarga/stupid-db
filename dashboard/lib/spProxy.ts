/**
 * Shared proxy helper for Stille Post API routes.
 * Forwards requests to the Rust backend at /sp/* endpoints.
 */

export const dynamic = "force-dynamic";

const API_BASE = process.env.API_BASE || "http://localhost:3088";

/** Proxy a request to the Rust backend and return the response. */
export async function spProxy(
  backendPath: string,
  req: Request,
  { forwardBody = false }: { forwardBody?: boolean } = {},
): Promise<Response> {
  // Forward query params for GET requests with filters
  const url = new URL(req.url);
  const query = url.search; // includes leading '?'

  const headers: Record<string, string> = {
    "Content-Type": "application/json",
  };

  const init: RequestInit = {
    method: req.method,
    headers,
  };

  if (forwardBody && (req.method === "POST" || req.method === "PUT" || req.method === "PATCH")) {
    init.body = await req.text();
  }

  const res = await fetch(`${API_BASE}${backendPath}${query}`, init);

  // Stream the response body through without buffering
  return new Response(res.body, {
    status: res.status,
    headers: { "Content-Type": res.headers.get("Content-Type") || "application/json" },
  });
}
