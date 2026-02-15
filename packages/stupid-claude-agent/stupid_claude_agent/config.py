"""Configuration management using pydantic-settings."""

from pathlib import Path
from typing import Literal

from pydantic import Field
from pydantic_settings import BaseSettings, SettingsConfigDict


class Settings(BaseSettings):
    """Application settings loaded from .env and environment."""

    model_config = SettingsConfigDict(
        env_file=".env",
        env_file_encoding="utf-8",
        case_sensitive=False,
        extra="ignore",
    )

    # Server
    host: str = Field(default="0.0.0.0", description="API server host")
    port: int = Field(default=8000, description="API server port")
    cors_origin: str = Field(default="*", description="CORS allowed origins")

    # Claude Code SDK
    claude_project_root: Path = Field(
        default_factory=lambda: Path(__file__).parent.parent.parent.parent,
        description="Root of the stupid-db project",
    )

    # LLM (for agent execution)
    llm_provider: Literal["openai", "anthropic", "ollama"] = Field(
        default="anthropic", description="LLM provider for agents"
    )
    anthropic_api_key: str = Field(default="", description="Anthropic API key")
    anthropic_model: str = Field(default="claude-sonnet-4-5-20250929", description="Claude model")
    openai_api_key: str = Field(default="", description="OpenAI API key")
    openai_model: str = Field(default="gpt-4o", description="OpenAI model")
    ollama_url: str = Field(default="http://localhost:11434", description="Ollama URL")
    ollama_model: str = Field(default="llama3.2", description="Ollama model")

    # Agent execution
    max_concurrent_agents: int = Field(default=3, description="Max concurrent agent executions")
    agent_timeout_seconds: int = Field(default=300, description="Agent execution timeout")

    @property
    def agents_dir(self) -> Path:
        """Path to the .claude/agents directory."""
        return self.claude_project_root / "packages" / "stupid-claude-agent" / ".claude" / "agents"

    @property
    def skills_dir(self) -> Path:
        """Path to the .claude/skills directory."""
        return self.claude_project_root / "packages" / "stupid-claude-agent" / ".claude" / "skills"


settings = Settings()
