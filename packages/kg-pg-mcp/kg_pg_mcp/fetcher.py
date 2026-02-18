"""URL fetcher using httpx."""

from __future__ import annotations

from dataclasses import dataclass

import httpx


@dataclass
class FetchResult:
    content: str | bytes
    content_type: str
    url: str
    title: str | None = None


async def fetch_url(url: str, timeout: float = 30.0) -> FetchResult:
    """Fetch a URL and return its content with content-type."""
    async with httpx.AsyncClient(
        follow_redirects=True,
        timeout=timeout,
        headers={"User-Agent": "kg-pg-mcp/0.1.0"},
    ) as client:
        response = await client.get(url)
        response.raise_for_status()

    content_type = response.headers.get("content-type", "text/plain").split(";")[0].strip()

    # Binary content (PDF, images) → return bytes
    if "pdf" in content_type or "octet-stream" in content_type:
        return FetchResult(
            content=response.content,
            content_type=content_type,
            url=str(response.url),
        )

    return FetchResult(
        content=response.text,
        content_type=content_type,
        url=str(response.url),
    )
