import { spProxy } from "@/lib/spProxy";
export const dynamic = "force-dynamic";
export function POST(req: Request) {
  return spProxy("/sp/import", req, { forwardBody: true });
}
