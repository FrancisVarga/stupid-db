-- Stille Post: pipeline execution runs, step results, reports, and delivery channels

-- Pipeline runs: tracks each execution of a pipeline
CREATE TABLE IF NOT EXISTS sp_runs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID,
    schedule_id UUID,
    status TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'running', 'completed', 'failed', 'cancelled')),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,
    error TEXT,
    trigger_type TEXT NOT NULL CHECK (trigger_type IN ('manual', 'scheduled', 'event')),
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_sp_runs_status ON sp_runs(status);
CREATE INDEX IF NOT EXISTS idx_sp_runs_pipeline ON sp_runs(pipeline_id);
CREATE INDEX IF NOT EXISTS idx_sp_runs_created ON sp_runs(created_at);

-- Step results: per-step execution details within a run
CREATE TABLE IF NOT EXISTS sp_step_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES sp_runs(id) ON DELETE CASCADE,
    step_id UUID,
    agent_id UUID,
    input_data JSONB,
    output_data JSONB,
    tokens_used INT,
    duration_ms INT,
    status TEXT NOT NULL DEFAULT 'pending'
);

CREATE INDEX IF NOT EXISTS idx_sp_step_results_run ON sp_step_results(run_id);

-- Reports: generated output from pipeline runs
CREATE TABLE IF NOT EXISTS sp_reports (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    run_id UUID NOT NULL REFERENCES sp_runs(id) ON DELETE CASCADE,
    title TEXT NOT NULL,
    content_html TEXT,
    content_json JSONB,
    render_blocks JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_sp_reports_run ON sp_reports(run_id);
CREATE INDEX IF NOT EXISTS idx_sp_reports_created ON sp_reports(created_at);

-- Deliveries: channel configuration for scheduled report distribution
CREATE TABLE IF NOT EXISTS sp_deliveries (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    schedule_id UUID,
    channel TEXT NOT NULL CHECK (channel IN ('email', 'webhook', 'telegram')),
    config_json JSONB NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT true
);

CREATE INDEX IF NOT EXISTS idx_sp_deliveries_schedule ON sp_deliveries(schedule_id);
