export const dynamic = "force-dynamic";
import { ingestionProxy } from "@/lib/ingestionProxy";

export async function POST(req: Request): Promise<Response> {
  return ingestionProxy("/api/ingestion/upload", req, { forwardBody: true });
}
