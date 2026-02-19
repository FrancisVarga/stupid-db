-- Add unique constraint on sp_agents.name for seed-on-startup upsert (ON CONFLICT).
-- The existing idx_sp_agents_name is a plain B-tree index; replace with UNIQUE.
DROP INDEX IF EXISTS idx_sp_agents_name;
CREATE UNIQUE INDEX IF NOT EXISTS idx_sp_agents_name ON sp_agents(name);
