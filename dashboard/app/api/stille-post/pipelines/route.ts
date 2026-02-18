export const dynamic = "force-dynamic";
import { spProxy } from "@/lib/spProxy";

export async function GET(req: Request): Promise<Response> {
  return spProxy("/sp/pipelines", req);
}

export async function POST(req: Request): Promise<Response> {
  return spProxy("/sp/pipelines", req, { forwardBody: true });
}
