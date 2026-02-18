-- Stille Post: data source definitions for report pipelines
CREATE TABLE IF NOT EXISTS sp_data_sources (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    source_type TEXT NOT NULL CHECK (source_type IN ('athena', 's3', 'api', 'upload')),
    config_json JSONB NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for filtering by source type
CREATE INDEX IF NOT EXISTS idx_sp_data_sources_type ON sp_data_sources(source_type);
