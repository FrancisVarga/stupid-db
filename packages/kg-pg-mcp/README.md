# kg-pg-mcp

Knowledge Graph MCP server backed by PostgreSQL + pgvector. Embed text, URLs, and files into a vector database and search with semantic similarity.

## Tools

| Tool | Description |
|------|-------------|
| `embed_text` | Embed arbitrary text with optional title, category, and metadata |
| `embed_url` | Fetch and embed a URL (or use pre-extracted content) |
| `embed_file` | Read and embed a local file (text, markdown, HTML, PDF, code) |
| `search` | Semantic similarity search with namespace, category, and JSONB metadata filters |
| `list_documents` | List stored documents with chunk counts and token totals |
| `delete_document` | Remove a document and all its chunks |
| `list_namespaces` | Show all namespaces with document counts |

## Stack

- **Framework**: [FastMCP](https://github.com/jlowin/fastmcp) (stdio + SSE transport)
- **Database**: PostgreSQL with [pgvector](https://github.com/pgvector/pgvector) HNSW cosine index
- **Embeddings**: OpenAI (`text-embedding-3-small` / `text-embedding-3-large` via Matryoshka truncation)
- **ORM**: SQLAlchemy 2.0 async + asyncpg
- **Migrations**: Alembic (async)
- **Chunking**: Heading-aware, token-based with configurable overlap

## Setup

```bash
# From the package directory
cd packages/kg-pg-mcp

# Install with uv (registered in workspace)
uv pip install -e .

# Copy and fill env
cp .env.example .env
# Edit .env — set KG_DATABASE_URL and KG_OPENAI_API_KEY

# Run migrations
python -m alembic upgrade head

# Start (stdio for Claude Code)
kg-pg-mcp

# Start (SSE for network access)
KG_TRANSPORT=sse kg-pg-mcp
```

## Docker

```bash
docker compose up --build
```

Runs in SSE mode on port `12312`. Requires a `.env` file with `KG_DATABASE_URL` pointing to an accessible PostgreSQL instance (not bundled).

## Configuration

All env vars use the `KG_` prefix. See [`.env.example`](.env.example) for the full list.

| Variable | Default | Description |
|----------|---------|-------------|
| `KG_DATABASE_URL` | `postgresql+asyncpg://localhost:5432/knowledge_graph` | PostgreSQL connection string |
| `KG_OPENAI_API_KEY` | — | OpenAI API key (required) |
| `KG_EMBEDDING_MODEL` | `text-embedding-3-small` | Embedding model (`text-embedding-3-small`, `text-embedding-3-large`) |
| `KG_CHUNK_SIZE` | `512` | Tokens per chunk |
| `KG_CHUNK_OVERLAP` | `50` | Overlap tokens between chunks |
| `KG_DEFAULT_NAMESPACE` | `default` | Default namespace for isolation |
| `KG_TRANSPORT` | `stdio` | Server transport (`stdio` or `sse`) |
| `KG_HOST` | `0.0.0.0` | SSE bind address |
| `KG_PORT` | `12312` | SSE port |

## Claude Code integration

Add to `.mcp.json`:

```json
{
  "mcpServers": {
    "kg-pg-mcp": {
      "command": "kg-pg-mcp",
      "env": {
        "KG_DATABASE_URL": "postgresql+asyncpg://user:pass@localhost:5432/knowledge_graph",
        "KG_OPENAI_API_KEY": "sk-..."
      }
    }
  }
}
```

## Namespaces

Namespaces isolate knowledge domains. Every embed and search call accepts an optional `namespace` parameter. Use them to separate projects, tenants, or topics without needing separate databases.

## Embedding dimensions

`text-embedding-3-large` natively outputs 3072 dimensions, which exceeds pgvector's 2000-dimension HNSW/IVFFlat index limit. The server automatically caps at 2000 dimensions using OpenAI's Matryoshka truncation (`dimensions` API parameter). This is transparent — no configuration needed.
