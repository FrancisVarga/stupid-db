export const dynamic = "force-dynamic";
import { ingestionProxy } from "@/lib/ingestionProxy";

export async function GET(req: Request): Promise<Response> {
  return ingestionProxy("/api/ingestion/sources", req);
}

export async function POST(req: Request): Promise<Response> {
  return ingestionProxy("/api/ingestion/sources", req, { forwardBody: true });
}
