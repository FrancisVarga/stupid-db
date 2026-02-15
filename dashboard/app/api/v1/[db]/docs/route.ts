import { NextRequest, NextResponse } from "next/server";

export const dynamic = "force-dynamic";

export async function GET(
  req: NextRequest,
  { params }: { params: Promise<{ db: string }> },
): Promise<Response> {
  const { db } = await params;
  const proto = req.headers.get("x-forwarded-proto") ?? "http";
  const host = req.headers.get("host") ?? "localhost:3000";
  const specUrl = `${proto}://${host}/api/v1/${db}/openapi.json`;

  const html = `<!DOCTYPE html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>stupid-db API: ${escapeHtml(db)}</title>
  <style>
    body { margin: 0; background: #0a0e14; }
  </style>
</head>
<body>
  <script
    id="api-reference"
    data-url="${escapeHtml(specUrl)}"
    data-configuration="${escapeHtml(JSON.stringify({
      theme: "deepSpace",
      layout: "modern",
      hideDownloadButton: false,
      hiddenClients: [],
      defaultHttpClient: { targetKey: "javascript", clientKey: "fetch" },
    }))}"
  ></script>
  <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
</body>
</html>`;

  return new NextResponse(html, {
    headers: {
      "Content-Type": "text/html; charset=utf-8",
      "Cache-Control": "public, max-age=300",
    },
  });
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}
