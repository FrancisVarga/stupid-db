"""OpenAI embedding client with batch support and token counting."""

from __future__ import annotations

from typing import TYPE_CHECKING, Any, Protocol, runtime_checkable

import tiktoken
from loguru import logger
from openai import AsyncOpenAI

from kg_pg_mcp.config import settings

if TYPE_CHECKING:
    pass


@runtime_checkable
class ProgressLike(Protocol):
    """Minimal progress protocol compatible with docket.Progress / fastmcp.Progress."""

    async def increment(self, amount: int = 1) -> Any: ...
    async def set_message(self, message: str) -> Any: ...


class Embedder:
    """Async embedding client wrapping OpenAI's embedding API."""

    def __init__(self, api_key: str | None = None, model: str | None = None) -> None:
        self.client = AsyncOpenAI(api_key=api_key or settings.openai_api_key)
        self.model = model or settings.embedding_model
        self.dimensions = settings.embedding_dimensions
        self.batch_size = settings.embedding_batch_size
        self._encoding: tiktoken.Encoding | None = None

    @property
    def encoding(self) -> tiktoken.Encoding:
        if self._encoding is None:
            try:
                self._encoding = tiktoken.encoding_for_model(self.model)
            except KeyError:
                self._encoding = tiktoken.get_encoding("cl100k_base")
        return self._encoding

    def count_tokens(self, text: str) -> int:
        """Count tokens in a text string."""
        return len(self.encoding.encode(text))

    async def _embed_batch(self, texts: list[str]) -> list[list[float]]:
        """Embed a single batch of texts via the OpenAI API."""
        kwargs: dict = {"input": texts, "model": self.model}
        if self.dimensions != settings.native_dimensions:
            kwargs["dimensions"] = self.dimensions

        response = await self.client.embeddings.create(**kwargs)
        sorted_data = sorted(response.data, key=lambda x: x.index)
        return [item.embedding for item in sorted_data]

    async def embed_texts(
        self,
        texts: list[str],
        progress: ProgressLike | None = None,
    ) -> list[list[float]]:
        """Embed multiple texts, automatically batching to stay within API limits.

        Args:
            texts: List of text strings to embed.
            progress: Optional progress reporter (fastmcp.Progress or any object
                      with increment() and set_message() async methods).

        Returns:
            List of embedding vectors, one per input text, in the same order.
        """
        if not texts:
            return []

        total = len(texts)

        # Small enough for a single call — skip batching overhead
        if total <= self.batch_size:
            result = await self._embed_batch(texts)
            if progress:
                await progress.increment(total)
            return result

        # Batch the API calls
        all_embeddings: list[list[float]] = []
        completed = 0

        for i in range(0, total, self.batch_size):
            batch = texts[i : i + self.batch_size]
            batch_num = i // self.batch_size + 1
            total_batches = (total + self.batch_size - 1) // self.batch_size
            logger.info(f"Embedding batch {batch_num}/{total_batches} ({len(batch)} chunks)")

            batch_embeddings = await self._embed_batch(batch)
            all_embeddings.extend(batch_embeddings)

            completed += len(batch)
            if progress:
                await progress.increment(len(batch))
                await progress.set_message(
                    f"Embedded {completed}/{total} chunks (batch {batch_num}/{total_batches})"
                )

        return all_embeddings

    async def embed_query(self, text: str) -> list[float]:
        """Embed a single query text. Convenience wrapper."""
        results = await self.embed_texts([text])
        return results[0]
