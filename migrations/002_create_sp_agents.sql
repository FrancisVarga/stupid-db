-- Stille Post: AI agent definitions for report generation
CREATE TABLE IF NOT EXISTS sp_agents (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    system_prompt TEXT NOT NULL,
    model TEXT NOT NULL DEFAULT 'claude-sonnet-4-6',
    skills_config JSONB DEFAULT '[]',
    mcp_servers_config JSONB DEFAULT '[]',
    tools_config JSONB DEFAULT '[]',
    template_id TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for agent lookup by name
CREATE INDEX IF NOT EXISTS idx_sp_agents_name ON sp_agents(name);

-- Index for chronological listing
CREATE INDEX IF NOT EXISTS idx_sp_agents_created_at ON sp_agents(created_at);
