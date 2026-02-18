-- 007: Add cross-table foreign keys for Stille Post tables
-- These are added separately because the tables are created in different migrations
-- and we need all tables to exist before adding cross-references.

-- sp_pipeline_steps.agent_id → sp_agents(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_pipeline_steps_agent'
  ) THEN
    ALTER TABLE sp_pipeline_steps
      ADD CONSTRAINT fk_sp_pipeline_steps_agent
      FOREIGN KEY (agent_id) REFERENCES sp_agents(id);
  END IF;
END $$;

-- sp_pipeline_steps.data_source_id → sp_data_sources(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_pipeline_steps_data_source'
  ) THEN
    ALTER TABLE sp_pipeline_steps
      ADD CONSTRAINT fk_sp_pipeline_steps_data_source
      FOREIGN KEY (data_source_id) REFERENCES sp_data_sources(id);
  END IF;
END $$;

-- sp_runs.pipeline_id → sp_pipelines(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_runs_pipeline'
  ) THEN
    ALTER TABLE sp_runs
      ADD CONSTRAINT fk_sp_runs_pipeline
      FOREIGN KEY (pipeline_id) REFERENCES sp_pipelines(id);
  END IF;
END $$;

-- sp_runs.schedule_id → sp_schedules(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_runs_schedule'
  ) THEN
    ALTER TABLE sp_runs
      ADD CONSTRAINT fk_sp_runs_schedule
      FOREIGN KEY (schedule_id) REFERENCES sp_schedules(id);
  END IF;
END $$;

-- sp_step_results.step_id → sp_pipeline_steps(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_step_results_step'
  ) THEN
    ALTER TABLE sp_step_results
      ADD CONSTRAINT fk_sp_step_results_step
      FOREIGN KEY (step_id) REFERENCES sp_pipeline_steps(id);
  END IF;
END $$;

-- sp_step_results.agent_id → sp_agents(id)
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_step_results_agent'
  ) THEN
    ALTER TABLE sp_step_results
      ADD CONSTRAINT fk_sp_step_results_agent
      FOREIGN KEY (agent_id) REFERENCES sp_agents(id);
  END IF;
END $$;

-- sp_deliveries.schedule_id → sp_schedules(id) ON DELETE CASCADE
DO $$
BEGIN
  IF NOT EXISTS (
    SELECT 1 FROM information_schema.table_constraints
    WHERE constraint_name = 'fk_sp_deliveries_schedule'
  ) THEN
    ALTER TABLE sp_deliveries
      ADD CONSTRAINT fk_sp_deliveries_schedule
      FOREIGN KEY (schedule_id) REFERENCES sp_schedules(id) ON DELETE CASCADE;
  END IF;
END $$;
