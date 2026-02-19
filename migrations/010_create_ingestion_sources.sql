-- Ingestion sources table: tracks configured data ingestion endpoints
-- with scheduling, ZMQ granularity, and run tracking.
CREATE TABLE IF NOT EXISTS ingestion_sources (
    id              UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    name            TEXT        NOT NULL,
    source_type     TEXT        NOT NULL CHECK (source_type IN ('parquet', 'directory', 's3', 'csv_json', 'push', 'queue')),
    config_json     JSONB       NOT NULL DEFAULT '{}',
    zmq_granularity TEXT        NOT NULL DEFAULT 'summary' CHECK (zmq_granularity IN ('summary', 'batched')),
    schedule_json   JSONB,
    enabled         BOOLEAN     NOT NULL DEFAULT true,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    last_run_at     TIMESTAMPTZ,
    next_run_at     TIMESTAMPTZ
);

CREATE INDEX IF NOT EXISTS idx_ingestion_sources_next_run
    ON ingestion_sources(next_run_at)
    WHERE enabled = true AND schedule_json IS NOT NULL;

CREATE INDEX IF NOT EXISTS idx_ingestion_sources_type
    ON ingestion_sources(source_type);

CREATE UNIQUE INDEX IF NOT EXISTS idx_ingestion_sources_name
    ON ingestion_sources(name);
