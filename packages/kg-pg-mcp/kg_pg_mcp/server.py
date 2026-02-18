"""FastMCP server with knowledge embedding, search tools, and skills provider."""

from __future__ import annotations

import os
import uuid
from contextlib import asynccontextmanager
from pathlib import Path
from typing import Any

from fastmcp import Context, FastMCP
from fastmcp.dependencies import Progress
from fastmcp.server.providers.skills import SkillsDirectoryProvider
from loguru import logger
from sqlalchemy import delete, func, select, text
from sqlalchemy.ext.asyncio import AsyncSession

from kg_pg_mcp.chunker import Chunker
from kg_pg_mcp.config import settings
from kg_pg_mcp.db import dispose_engine, get_engine, get_session_factory
from kg_pg_mcp.embedder import Embedder
from kg_pg_mcp.fetcher import fetch_url
from kg_pg_mcp.file_reader import read_file
from kg_pg_mcp.models import Base, KBChunk, KBDocument
from kg_pg_mcp.parsers import ParseResult, detect_content_type, parse_content

# Configure Docket backend before FastMCP init picks it up
os.environ.setdefault("FASTMCP_DOCKET_URL", settings.docket_url)


_embedder: Embedder | None = None
_chunker: Chunker | None = None


def get_embedder() -> Embedder:
    global _embedder
    if _embedder is None:
        _embedder = Embedder()
    return _embedder


def get_chunker() -> Chunker:
    global _chunker
    if _chunker is None:
        _chunker = Chunker()
    return _chunker


@asynccontextmanager
async def lifespan(server: FastMCP):
    """Manage database lifecycle."""
    logger.info("Starting kg-pg-mcp server")

    # Create tables if they don't exist
    eng = get_engine()
    async with eng.begin() as conn:
        await conn.execute(text("CREATE EXTENSION IF NOT EXISTS vector"))
        await conn.run_sync(Base.metadata.create_all)

    yield {}

    await dispose_engine()
    logger.info("kg-pg-mcp server stopped")


mcp = FastMCP(
    "kg-pg-mcp",
    instructions="Knowledge Graph MCP server. Embed text, URLs, and files into a PostgreSQL vector database. Search with semantic similarity and metadata filters. Use namespaces to isolate different knowledge domains.",
    version="0.1.0",
    lifespan=lifespan,
    tasks=True,
)


# ── Skills Provider ─────────────────────────────────

if settings.skills_roots:
    roots = [Path(r).expanduser().resolve() for r in settings.skills_roots]
    valid_roots = [r for r in roots if r.is_dir()]
    if valid_roots:
        mcp.add_provider(
            SkillsDirectoryProvider(
                roots=valid_roots,
                supporting_files=settings.skills_supporting_files,
            )
        )
        logger.info(f"Skills provider loaded: {len(valid_roots)} root(s)")
    else:
        logger.warning(f"Skills roots configured but none exist: {settings.skills_roots}")


# ── Helpers ──────────────────────────────────────────


async def _store_document(
    session: AsyncSession,
    embedder: Embedder,
    chunker: Chunker,
    parsed: ParseResult,
    source_type: str,
    source_ref: str | None,
    namespace: str,
    category: str | None,
    metadata: dict | None,
    ctx: Context | None = None,
    progress: Any | None = None,
) -> dict[str, Any]:
    """Chunk, embed, and store a parsed document."""
    chunks = chunker.chunk(parsed.text)
    total_tokens = sum(c.token_count for c in chunks)

    if ctx:
        await ctx.info(f"Chunked into {len(chunks)} pieces ({total_tokens} tokens)")

    # Set up progress tracking
    if progress:
        await progress.set_total(len(chunks))
        await progress.set_message("Embedding chunks")

    # Batch embed all chunks (auto-batched for large documents)
    texts = [c.content for c in chunks]
    embeddings = await embedder.embed_texts(texts, progress=progress)

    if progress:
        await progress.set_message("Storing in database")

    # Create document record
    doc = KBDocument(
        namespace=namespace,
        title=parsed.title or source_ref or "Untitled",
        source_type=source_type,
        source_ref=source_ref,
        category=category,
        metadata_=metadata or parsed.metadata,
        token_count=total_tokens,
    )
    session.add(doc)
    await session.flush()  # get doc.id

    # Create chunk records
    for chunk_result, embedding in zip(chunks, embeddings):
        chunk = KBChunk(
            document_id=doc.id,
            namespace=namespace,
            chunk_index=chunk_result.chunk_index,
            content=chunk_result.content,
            section_heading=chunk_result.section_heading,
            token_count=chunk_result.token_count,
            embedding=embedding,
        )
        session.add(chunk)

    await session.commit()

    return {
        "document_id": str(doc.id),
        "title": doc.title,
        "namespace": namespace,
        "chunks": len(chunks),
        "total_tokens": total_tokens,
        "category": category,
    }


# ── MCP Tools ────────────────────────────────────────


@mcp.tool(task=True)
async def embed_text(
    text: str,
    ctx: Context,
    progress: Progress = Progress(),
    namespace: str | None = None,
    category: str | None = None,
    title: str | None = None,
    metadata: dict | None = None,
) -> dict:
    """Embed arbitrary text into the knowledge base.

    Args:
        text: The text content to embed.
        namespace: Isolation namespace (default: "default").
        category: Optional category for filtering.
        title: Optional title for the document.
        metadata: Optional JSON metadata to store with the document.
    """
    ns = namespace or settings.default_namespace
    embedder = get_embedder()
    chunker = get_chunker()

    parsed = ParseResult(text=text, title=title)

    async with progress, get_session_factory()() as session:
        return await _store_document(
            session, embedder, chunker, parsed,
            source_type="text", source_ref=None,
            namespace=ns, category=category, metadata=metadata,
            ctx=ctx, progress=progress,
        )


@mcp.tool(task=True)
async def embed_url(
    url: str,
    ctx: Context,
    progress: Progress = Progress(),
    namespace: str | None = None,
    category: str | None = None,
    metadata: dict | None = None,
    content: str | None = None,
) -> dict:
    """Embed a URL's content into the knowledge base.

    Can fetch the URL automatically or use pre-extracted content.

    Args:
        url: The URL to embed.
        namespace: Isolation namespace (default: "default").
        category: Optional category for filtering.
        metadata: Optional JSON metadata.
        content: Pre-extracted content (skips fetching if provided).
    """
    ns = namespace or settings.default_namespace
    embedder = get_embedder()
    chunker = get_chunker()

    if content:
        parsed = await parse_content(content, content_type="text", source_path=url)
    else:
        await ctx.info(f"Fetching {url}")
        result = await fetch_url(url)
        ct = detect_content_type(content_type=result.content_type)
        parsed = await parse_content(result.content, content_type=ct, source_path=url)

    async with progress, get_session_factory()() as session:
        return await _store_document(
            session, embedder, chunker, parsed,
            source_type="url", source_ref=url,
            namespace=ns, category=category, metadata=metadata,
            ctx=ctx, progress=progress,
        )


@mcp.tool(task=True)
async def embed_file(
    file_path: str,
    ctx: Context,
    progress: Progress = Progress(),
    namespace: str | None = None,
    category: str | None = None,
    metadata: dict | None = None,
    content: str | None = None,
) -> dict:
    """Embed a local file's content into the knowledge base.

    Can read the file automatically or use pre-extracted content.

    Args:
        file_path: Path to the local file.
        namespace: Isolation namespace (default: "default").
        category: Optional category for filtering.
        metadata: Optional JSON metadata.
        content: Pre-extracted content (skips file reading if provided).
    """
    ns = namespace or settings.default_namespace
    embedder = get_embedder()
    chunker = get_chunker()

    if content:
        ct = detect_content_type(path=file_path)
        parsed = await parse_content(content, content_type=ct, source_path=file_path)
    else:
        await ctx.info(f"Reading {file_path}")
        result = await read_file(file_path)
        parsed = await parse_content(result.content, content_type=result.content_type, source_path=file_path)

    async with progress, get_session_factory()() as session:
        return await _store_document(
            session, embedder, chunker, parsed,
            source_type="file", source_ref=file_path,
            namespace=ns, category=category, metadata=metadata,
            ctx=ctx, progress=progress,
        )


@mcp.tool()
async def search(
    query: str,
    ctx: Context,
    namespace: str | None = None,
    limit: int = 10,
    category: str | None = None,
    metadata_filter: dict | None = None,
) -> list[dict]:
    """Search the knowledge base using semantic similarity.

    Args:
        query: The search query text.
        namespace: Search within this namespace (default: "default").
        limit: Maximum results to return (default: 10).
        category: Filter by category.
        metadata_filter: JSON filter applied to document metadata.
    """
    ns = namespace or settings.default_namespace
    embedder = get_embedder()

    query_embedding = await embedder.embed_query(query)

    async with get_session_factory()() as session:
        # Build the similarity search query
        similarity = (1 - KBChunk.embedding.cosine_distance(query_embedding)).label("similarity")

        stmt = (
            select(
                KBChunk.id,
                KBChunk.document_id,
                KBChunk.content,
                KBChunk.chunk_index,
                KBChunk.section_heading,
                KBDocument.title,
                KBDocument.source_type,
                KBDocument.source_ref,
                KBDocument.category,
                similarity,
            )
            .join(KBDocument, KBChunk.document_id == KBDocument.id)
            .where(KBChunk.namespace == ns)
        )

        if category:
            stmt = stmt.where(KBDocument.category == category)

        if metadata_filter:
            # JSONB containment operator @>
            stmt = stmt.where(KBDocument.metadata_.op("@>")(metadata_filter))

        stmt = stmt.order_by(similarity.desc()).limit(limit)

        result = await session.execute(stmt)
        rows = result.all()

    return [
        {
            "chunk_id": str(row.id),
            "document_id": str(row.document_id),
            "title": row.title,
            "content": row.content,
            "chunk_index": row.chunk_index,
            "section_heading": row.section_heading,
            "source_type": row.source_type,
            "source_ref": row.source_ref,
            "category": row.category,
            "similarity": round(float(row.similarity), 4),
        }
        for row in rows
    ]


@mcp.tool()
async def list_documents(
    ctx: Context,
    namespace: str | None = None,
    category: str | None = None,
    source_type: str | None = None,
) -> list[dict]:
    """List documents stored in the knowledge base.

    Args:
        namespace: Filter by namespace (default: "default").
        category: Filter by category.
        source_type: Filter by source type (text, url, file).
    """
    ns = namespace or settings.default_namespace

    async with get_session_factory()() as session:
        stmt = (
            select(
                KBDocument.id,
                KBDocument.title,
                KBDocument.source_type,
                KBDocument.source_ref,
                KBDocument.category,
                KBDocument.token_count,
                KBDocument.created_at,
                func.count(KBChunk.id).label("chunk_count"),
            )
            .outerjoin(KBChunk, KBChunk.document_id == KBDocument.id)
            .where(KBDocument.namespace == ns)
            .group_by(KBDocument.id)
            .order_by(KBDocument.created_at.desc())
        )

        if category:
            stmt = stmt.where(KBDocument.category == category)
        if source_type:
            stmt = stmt.where(KBDocument.source_type == source_type)

        result = await session.execute(stmt)
        rows = result.all()

    return [
        {
            "document_id": str(row.id),
            "title": row.title,
            "source_type": row.source_type,
            "source_ref": row.source_ref,
            "category": row.category,
            "token_count": row.token_count,
            "chunk_count": row.chunk_count,
            "created_at": row.created_at.isoformat(),
        }
        for row in rows
    ]


@mcp.tool()
async def delete_document(document_id: str, ctx: Context) -> dict:
    """Delete a document and all its chunks from the knowledge base.

    Args:
        document_id: UUID of the document to delete.
    """
    doc_uuid = uuid.UUID(document_id)

    async with get_session_factory()() as session:
        # Delete chunks first (cascade should handle, but be explicit)
        await session.execute(
            delete(KBChunk).where(KBChunk.document_id == doc_uuid)
        )
        result = await session.execute(
            delete(KBDocument).where(KBDocument.id == doc_uuid)
        )
        await session.commit()

    deleted = result.rowcount > 0
    return {
        "deleted": deleted,
        "document_id": document_id,
    }


@mcp.tool()
async def list_namespaces(ctx: Context) -> list[dict]:
    """List all namespaces with document counts."""
    async with get_session_factory()() as session:
        stmt = (
            select(
                KBDocument.namespace,
                func.count(KBDocument.id).label("document_count"),
                func.sum(KBDocument.token_count).label("total_tokens"),
            )
            .group_by(KBDocument.namespace)
            .order_by(func.count(KBDocument.id).desc())
        )
        result = await session.execute(stmt)
        rows = result.all()

    return [
        {
            "namespace": row.namespace,
            "document_count": row.document_count,
            "total_tokens": int(row.total_tokens or 0),
        }
        for row in rows
    ]


# ── Entrypoint ───────────────────────────────────────


def main():
    """Run the MCP server."""
    if settings.transport == "sse":
        mcp.run(transport="sse", host=settings.host, port=settings.port)
    else:
        mcp.run(transport="stdio")


if __name__ == "__main__":
    main()
