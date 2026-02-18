"""Local file reader with content-type detection."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path

from kg_pg_mcp.parsers import detect_content_type


@dataclass
class FileResult:
    content: str | bytes
    file_path: str
    content_type: str
    file_size: int


async def read_file(file_path: str) -> FileResult:
    """Read a local file and detect its content type."""
    path = Path(file_path).expanduser().resolve()

    if not path.exists():
        raise FileNotFoundError(f"File not found: {path}")
    if not path.is_file():
        raise ValueError(f"Not a file: {path}")

    content_type = detect_content_type(path=str(path))
    file_size = path.stat().st_size

    # Binary files (PDF)
    if content_type == "pdf":
        content: str | bytes = path.read_bytes()
    else:
        content = path.read_text(encoding="utf-8", errors="replace")

    return FileResult(
        content=content,
        file_path=str(path),
        content_type=content_type,
        file_size=file_size,
    )
