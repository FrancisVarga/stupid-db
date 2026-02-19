export const dynamic = "force-dynamic";
import { ingestionProxy } from "@/lib/ingestionProxy";

export async function GET(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return ingestionProxy(`/api/ingestion/sources/${id}`, req);
}

export async function PUT(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return ingestionProxy(`/api/ingestion/sources/${id}`, req, { forwardBody: true });
}

export async function DELETE(
  req: Request,
  { params }: { params: Promise<{ id: string }> },
): Promise<Response> {
  const { id } = await params;
  return ingestionProxy(`/api/ingestion/sources/${id}`, req);
}
