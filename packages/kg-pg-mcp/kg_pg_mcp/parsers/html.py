"""HTML parser — strips scripts/styles, converts to markdown."""

from __future__ import annotations

from bs4 import BeautifulSoup
from markdownify import markdownify

from kg_pg_mcp.parsers import ParseResult


async def parse_html(content: str | bytes, source_path: str | None = None) -> ParseResult:
    """Parse HTML content into clean text via markdown conversion."""
    if isinstance(content, bytes):
        content = content.decode("utf-8", errors="replace")

    soup = BeautifulSoup(content, "html.parser")

    # Extract title
    title = None
    title_tag = soup.find("title")
    if title_tag:
        title = title_tag.get_text(strip=True)

    # Remove script and style elements
    for tag in soup(["script", "style", "nav", "footer", "header"]):
        tag.decompose()

    # Convert to markdown
    text = markdownify(str(soup), heading_style="ATX", strip=["img"])
    # Clean up excessive whitespace
    lines = [line.rstrip() for line in text.split("\n")]
    text = "\n".join(lines)
    # Collapse 3+ blank lines into 2
    while "\n\n\n" in text:
        text = text.replace("\n\n\n", "\n\n")

    return ParseResult(
        text=text.strip(),
        title=title,
        metadata={"format": "html", "original_length": len(content)},
    )
