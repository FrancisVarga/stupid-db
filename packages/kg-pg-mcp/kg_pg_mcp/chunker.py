"""Smart chunking pipeline — heading-aware, token-based, with overlap."""

from __future__ import annotations

import re
from dataclasses import dataclass

import tiktoken

from kg_pg_mcp.config import settings


@dataclass
class ChunkResult:
    content: str
    chunk_index: int
    section_heading: str | None = None
    token_count: int = 0


@dataclass
class ChunkerConfig:
    chunk_size: int = settings.chunk_size  # max tokens per chunk
    chunk_overlap: int = settings.chunk_overlap  # overlap tokens between chunks
    model: str = settings.embedding_model


class Chunker:
    """Token-based chunker with heading awareness and configurable overlap."""

    def __init__(self, config: ChunkerConfig | None = None) -> None:
        self.config = config or ChunkerConfig()
        try:
            self._encoding = tiktoken.encoding_for_model(self.config.model)
        except KeyError:
            self._encoding = tiktoken.get_encoding("cl100k_base")

    def count_tokens(self, text: str) -> int:
        return len(self._encoding.encode(text))

    def chunk(self, text: str) -> list[ChunkResult]:
        """Split text into chunks with heading awareness and token-based sizing."""
        if not text.strip():
            return []

        sections = self._split_by_headings(text)
        chunks: list[ChunkResult] = []
        chunk_index = 0

        for heading, section_text in sections:
            section_chunks = self._split_by_tokens(section_text)
            for chunk_text in section_chunks:
                chunks.append(
                    ChunkResult(
                        content=chunk_text,
                        chunk_index=chunk_index,
                        section_heading=heading,
                        token_count=self.count_tokens(chunk_text),
                    )
                )
                chunk_index += 1

        return chunks

    def _split_by_headings(self, text: str) -> list[tuple[str | None, str]]:
        """Split text into (heading, content) sections based on markdown headings."""
        heading_pattern = re.compile(r"^(#{1,6})\s+(.+)$", re.MULTILINE)
        matches = list(heading_pattern.finditer(text))

        if not matches:
            return [(None, text)]

        sections: list[tuple[str | None, str]] = []

        # Content before first heading
        if matches[0].start() > 0:
            pre_content = text[: matches[0].start()].strip()
            if pre_content:
                sections.append((None, pre_content))

        # Each heading and its content
        for i, match in enumerate(matches):
            heading = match.group(2).strip()
            start = match.end()
            end = matches[i + 1].start() if i + 1 < len(matches) else len(text)
            content = text[start:end].strip()
            if content:
                sections.append((heading, content))

        return sections

    def _split_by_tokens(self, text: str) -> list[str]:
        """Split text into token-sized chunks with overlap, respecting paragraph boundaries."""
        tokens = self._encoding.encode(text)
        total = len(tokens)

        if total <= self.config.chunk_size:
            return [text]

        chunks: list[str] = []
        start = 0

        while start < total:
            end = min(start + self.config.chunk_size, total)
            chunk_tokens = tokens[start:end]
            chunk_text = self._encoding.decode(chunk_tokens)

            # Try to break at a paragraph or sentence boundary
            if end < total:
                chunk_text = self._snap_to_boundary(chunk_text)

            chunks.append(chunk_text.strip())
            # Advance by chunk_size minus overlap
            start += self.config.chunk_size - self.config.chunk_overlap

        return [c for c in chunks if c]

    @staticmethod
    def _snap_to_boundary(text: str) -> str:
        """Try to snap the chunk end to a paragraph or sentence boundary."""
        # Try paragraph boundary (double newline)
        last_para = text.rfind("\n\n")
        if last_para > len(text) // 2:
            return text[: last_para + 1]

        # Try sentence boundary
        for sep in (". ", ".\n", "! ", "? "):
            last_sep = text.rfind(sep)
            if last_sep > len(text) // 2:
                return text[: last_sep + 1]

        return text
