export const dynamic = "force-dynamic";
import { spProxy } from "@/lib/spProxy";

export async function POST(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return spProxy(`/sp/deliveries/${id}/test`, req, { forwardBody: true });
}
