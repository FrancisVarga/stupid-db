import { spProxy } from "@/lib/spProxy";
export const dynamic = "force-dynamic";
export function GET(req: Request) {
  return spProxy("/sp/export", req);
}
