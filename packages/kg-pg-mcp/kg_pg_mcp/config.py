"""Configuration via pydantic-settings — reads .env + environment variables.

All env vars are prefixed with KG_ to avoid clashing with the root project's
DATABASE_URL and other variables. Example: KG_DATABASE_URL, KG_OPENAI_API_KEY.
"""

from pathlib import Path

from pydantic_settings import BaseSettings, SettingsConfigDict

# Resolve .env relative to the package root (where pyproject.toml lives)
_PACKAGE_ROOT = Path(__file__).resolve().parent.parent
_ENV_FILE = _PACKAGE_ROOT / ".env"


# Native dimensions for known OpenAI embedding models
MODEL_NATIVE_DIMENSIONS: dict[str, int] = {
    "text-embedding-3-small": 1536,
    "text-embedding-3-large": 3072,
    "text-embedding-ada-002": 1536,
}

# pgvector HNSW/IVFFlat index limit
MAX_INDEX_DIMENSIONS = 2000


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=str(_ENV_FILE),
        env_file_encoding="utf-8",
        env_prefix="KG_",
        extra="ignore",
    )

    # Database
    database_url: str = "postgresql+asyncpg://localhost:5432/knowledge_graph"

    # OpenAI
    openai_api_key: str = ""
    embedding_model: str = "text-embedding-3-small"

    # Chunking
    chunk_size: int = 512  # tokens
    chunk_overlap: int = 50  # tokens

    # Namespace
    default_namespace: str = "default"

    # Server
    transport: str = "stdio"  # stdio | sse
    host: str = "0.0.0.0"
    port: int = 12312

    @property
    def native_dimensions(self) -> int:
        """Native dimensions for the configured model."""
        return MODEL_NATIVE_DIMENSIONS.get(self.embedding_model, 1536)

    @property
    def embedding_dimensions(self) -> int:
        """Effective dimensions — capped at pgvector index limit (2000).

        text-embedding-3-large natively outputs 3072 dims, but pgvector
        HNSW/IVFFlat indexes cap at 2000. OpenAI's v3 models support
        Matryoshka truncation via the `dimensions` API parameter.
        """
        return min(self.native_dimensions, MAX_INDEX_DIMENSIONS)


settings = Settings()
