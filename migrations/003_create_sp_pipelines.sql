-- Stille Post: pipeline DAG definitions for multi-agent workflows
CREATE TABLE IF NOT EXISTS sp_pipelines (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Pipeline steps: ordered agent invocations within a pipeline
CREATE TABLE IF NOT EXISTS sp_pipeline_steps (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    pipeline_id UUID NOT NULL REFERENCES sp_pipelines(id) ON DELETE CASCADE,
    agent_id UUID,  -- references sp_agents(id), added as FK after sp_agents table exists
    step_order INT NOT NULL,
    input_mapping JSONB NOT NULL DEFAULT '{}',
    output_mapping JSONB NOT NULL DEFAULT '{}',
    parallel_group INT,
    data_source_id UUID,
    UNIQUE(pipeline_id, step_order)
);

-- Index for step lookup by pipeline
CREATE INDEX IF NOT EXISTS idx_sp_pipeline_steps_pipeline ON sp_pipeline_steps(pipeline_id);
