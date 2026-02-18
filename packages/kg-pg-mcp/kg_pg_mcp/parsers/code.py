"""Code parser with language detection."""

from __future__ import annotations

from pathlib import Path

from kg_pg_mcp.parsers import ParseResult

# Map file extensions to language names
EXT_TO_LANG: dict[str, str] = {
    ".py": "python",
    ".rs": "rust",
    ".ts": "typescript",
    ".tsx": "typescript",
    ".js": "javascript",
    ".jsx": "javascript",
    ".java": "java",
    ".go": "go",
    ".c": "c",
    ".cpp": "cpp",
    ".h": "c",
    ".rb": "ruby",
    ".sh": "bash",
    ".yaml": "yaml",
    ".yml": "yaml",
    ".toml": "toml",
    ".json": "json",
    ".sql": "sql",
}


async def parse_code(content: str | bytes, source_path: str | None = None) -> ParseResult:
    """Parse source code, detecting language from file extension."""
    if isinstance(content, bytes):
        content = content.decode("utf-8", errors="replace")

    language = "unknown"
    if source_path:
        ext = Path(source_path).suffix.lower()
        language = EXT_TO_LANG.get(ext, "unknown")

    # Wrap in fenced code block for chunker to treat as atomic
    wrapped = f"```{language}\n{content}\n```"

    title = Path(source_path).name if source_path else None

    return ParseResult(
        text=wrapped,
        title=title,
        metadata={"format": "code", "language": language},
        format_hints={"language": language},
    )
