-- Stille Post: cron-based scheduling for pipeline execution
CREATE TABLE IF NOT EXISTS sp_schedules (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES sp_pipelines(id) ON DELETE CASCADE,
    cron_expression TEXT NOT NULL,
    timezone TEXT NOT NULL DEFAULT 'UTC',
    enabled BOOLEAN NOT NULL DEFAULT true,
    last_run_at TIMESTAMPTZ,
    next_run_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Index for scheduler polling: find next runnable schedules
CREATE INDEX IF NOT EXISTS idx_sp_schedules_next_run ON sp_schedules(next_run_at) WHERE enabled = true;

-- Index for pipeline lookup
CREATE INDEX IF NOT EXISTS idx_sp_schedules_pipeline ON sp_schedules(pipeline_id);
