"""Content parsers — detect format and extract text from various file types."""

from __future__ import annotations

from dataclasses import dataclass, field
from pathlib import Path


@dataclass
class ParseResult:
    text: str
    title: str | None = None
    metadata: dict = field(default_factory=dict)
    format_hints: dict = field(default_factory=dict)  # e.g. {"language": "python"}


def detect_content_type(path: str | None = None, content_type: str | None = None) -> str:
    """Detect parser type from file extension or content-type header."""
    if content_type:
        ct = content_type.lower()
        if "html" in ct:
            return "html"
        if "pdf" in ct:
            return "pdf"
        if "json" in ct or "javascript" in ct or "typescript" in ct:
            return "code"
        if "markdown" in ct:
            return "text"
        return "text"

    if path:
        ext = Path(path).suffix.lower()
        ext_map = {
            ".html": "html",
            ".htm": "html",
            ".pdf": "pdf",
            ".py": "code",
            ".rs": "code",
            ".ts": "code",
            ".tsx": "code",
            ".js": "code",
            ".jsx": "code",
            ".java": "code",
            ".go": "code",
            ".c": "code",
            ".cpp": "code",
            ".h": "code",
            ".rb": "code",
            ".sh": "code",
            ".yaml": "code",
            ".yml": "code",
            ".toml": "code",
            ".json": "code",
            ".sql": "code",
            ".md": "text",
            ".txt": "text",
            ".rst": "text",
            ".csv": "text",
        }
        return ext_map.get(ext, "text")

    return "text"


async def parse_content(
    content: str | bytes,
    content_type: str = "text",
    source_path: str | None = None,
) -> ParseResult:
    """Route content to the appropriate parser."""
    from kg_pg_mcp.parsers.code import parse_code
    from kg_pg_mcp.parsers.html import parse_html
    from kg_pg_mcp.parsers.pdf import parse_pdf
    from kg_pg_mcp.parsers.text import parse_text

    parser_map = {
        "text": parse_text,
        "html": parse_html,
        "pdf": parse_pdf,
        "code": parse_code,
    }

    parser = parser_map.get(content_type, parse_text)
    return await parser(content, source_path=source_path)
