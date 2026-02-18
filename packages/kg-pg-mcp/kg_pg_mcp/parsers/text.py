"""Plain text and markdown parser."""

from __future__ import annotations

import re

from kg_pg_mcp.parsers import ParseResult


async def parse_text(content: str | bytes, source_path: str | None = None) -> ParseResult:
    """Parse plain text or markdown content."""
    if isinstance(content, bytes):
        content = content.decode("utf-8", errors="replace")

    # Try to extract a title from the first heading or first line
    title = None
    heading_match = re.match(r"^#{1,2}\s+(.+)$", content, re.MULTILINE)
    if heading_match:
        title = heading_match.group(1).strip()
    elif content.strip():
        first_line = content.strip().split("\n")[0][:120]
        title = first_line

    return ParseResult(
        text=content,
        title=title,
        metadata={"format": "text"},
    )
