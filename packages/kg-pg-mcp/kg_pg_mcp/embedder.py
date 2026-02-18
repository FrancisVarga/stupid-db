"""OpenAI embedding client with batch support and token counting."""

import tiktoken
from openai import AsyncOpenAI

from kg_pg_mcp.config import settings


class Embedder:
    """Async embedding client wrapping OpenAI's embedding API."""

    def __init__(self, api_key: str | None = None, model: str | None = None) -> None:
        self.client = AsyncOpenAI(api_key=api_key or settings.openai_api_key)
        self.model = model or settings.embedding_model
        self.dimensions = settings.embedding_dimensions
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

    async def embed_texts(self, texts: list[str]) -> list[list[float]]:
        """Embed multiple texts in a single API call.

        Returns list of embedding vectors, one per input text.
        """
        if not texts:
            return []

        # Pass dimensions to truncate for models that support Matryoshka
        # (text-embedding-3-*). Keeps vectors within pgvector's 2000-dim index limit.
        kwargs: dict = {"input": texts, "model": self.model}
        if self.dimensions != settings.native_dimensions:
            kwargs["dimensions"] = self.dimensions

        response = await self.client.embeddings.create(**kwargs)
        # Sort by index to preserve input order
        sorted_data = sorted(response.data, key=lambda x: x.index)
        return [item.embedding for item in sorted_data]

    async def embed_query(self, text: str) -> list[float]:
        """Embed a single query text. Convenience wrapper."""
        results = await self.embed_texts([text])
        return results[0]
