export const dynamic = "force-dynamic";
import { spProxy } from "@/lib/spProxy";

export async function GET(req: Request): Promise<Response> {
  return spProxy("/sp/agents", req);
}

export async function POST(req: Request): Promise<Response> {
  return spProxy("/sp/agents", req, { forwardBody: true });
}
