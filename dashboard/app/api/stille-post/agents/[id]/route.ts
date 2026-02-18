export const dynamic = "force-dynamic";
import { spProxy } from "@/lib/spProxy";

export async function GET(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return spProxy(`/sp/agents/${id}`, req);
}

export async function PUT(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return spProxy(`/sp/agents/${id}`, req, { forwardBody: true });
}

export async function DELETE(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return spProxy(`/sp/agents/${id}`, req);
}
