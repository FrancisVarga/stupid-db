export const dynamic = "force-dynamic";

const API_BASE = process.env.API_BASE || "http://localhost:3088";

export async function POST(req: Request): Promise<Response> {
  const yamlBody = await req.text();

  const res = await fetch(`${API_BASE}/rules/validate`, {
    method: "POST",
    headers: { "Content-Type": "application/yaml" },
    body: yamlBody,
  });

  const json = await res.json();
  return new Response(JSON.stringify(json), {
    status: res.status,
    headers: { "Content-Type": "application/json" },
  });
}
