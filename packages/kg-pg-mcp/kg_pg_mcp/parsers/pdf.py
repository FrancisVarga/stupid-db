"""PDF parser using PyMuPDF (fitz)."""

from __future__ import annotations

from kg_pg_mcp.parsers import ParseResult


async def parse_pdf(content: str | bytes, source_path: str | None = None) -> ParseResult:
    """Parse PDF content using PyMuPDF. Accepts bytes or a file path."""
    import fitz

    if isinstance(content, str) and source_path:
        # content is a path hint, open from source_path
        doc = fitz.open(source_path)
    elif isinstance(content, bytes):
        doc = fitz.open(stream=content, filetype="pdf")
    else:
        # Treat as file path string
        doc = fitz.open(content)

    pages_text: list[str] = []
    title = doc.metadata.get("title") if doc.metadata else None

    for page_num in range(len(doc)):
        page = doc[page_num]
        text = page.get_text("text")
        if text.strip():
            pages_text.append(f"[Page {page_num + 1}]\n{text}")

    doc.close()

    full_text = "\n\n".join(pages_text)

    return ParseResult(
        text=full_text,
        title=title or None,
        metadata={"format": "pdf", "page_count": len(pages_text)},
    )
