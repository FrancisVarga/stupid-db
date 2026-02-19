-- Prompts table: stores externalized LLM prompt templates.
-- Seeded from data/bundeswehr/prompts/*.md on startup.
CREATE TABLE IF NOT EXISTS prompts (
    name TEXT PRIMARY KEY,
    content TEXT NOT NULL,
    description TEXT NOT NULL DEFAULT '',
    placeholders TEXT[] NOT NULL DEFAULT '{}',
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
