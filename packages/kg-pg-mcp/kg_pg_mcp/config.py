"""Configuration via pydantic-settings — reads .env + environment variables."""

from pydantic_settings import BaseSettings, SettingsConfigDict


# Dimension lookup for known OpenAI embedding models
MODEL_DIMENSIONS: dict[str, int] = {
    "text-embedding-3-small": 1536,
    "text-embedding-3-large": 3072,
    "text-embedding-ada-002": 1536,
}


class Settings(BaseSettings):
    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
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
    port: int = 8100

    @property
    def embedding_dimensions(self) -> int:
        """Auto-detect embedding dimensions from model name."""
        return MODEL_DIMENSIONS.get(self.embedding_model, 1536)


settings = Settings()
