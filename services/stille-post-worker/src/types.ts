/** Matches sp_agents table */
export interface SpAgent {
  id: string;
  name: string;
  description: string | null;
  system_prompt: string;
  model: string;
  skills_config: unknown[];
  mcp_servers_config: unknown[];
  tools_config: unknown[];
  template_id: string | null;
  created_at: Date;
  updated_at: Date;
}

/** Matches sp_pipelines table */
export interface SpPipeline {
  id: string;
  name: string;
  description: string | null;
  created_at: Date;
  updated_at: Date;
}

/** Matches sp_pipeline_steps table */
export interface SpPipelineStep {
  id: string;
  pipeline_id: string;
  agent_id: string | null;
  step_order: number;
  input_mapping: Record<string, unknown>;
  output_mapping: Record<string, unknown>;
  parallel_group: number | null;
  data_source_id: string | null;
}

/** Matches sp_schedules table (planned) */
export interface SpSchedule {
  id: string;
  pipeline_id: string;
  cron_expr: string;
  enabled: boolean;
  last_run_at: Date | null;
  next_run_at: Date | null;
  created_at: Date;
  updated_at: Date;
}

/** Matches sp_data_sources table */
export interface SpDataSource {
  id: string;
  name: string;
  source_type: "athena" | "s3" | "api" | "upload";
  config_json: Record<string, unknown>;
  created_at: Date;
  updated_at: Date;
}

/** Matches sp_runs table (planned) */
export interface SpRun {
  id: string;
  pipeline_id: string;
  schedule_id: string | null;
  status: "pending" | "running" | "completed" | "failed";
  started_at: Date | null;
  completed_at: Date | null;
  error: string | null;
  created_at: Date;
}

/** Matches sp_step_results table (planned) */
export interface SpStepResult {
  id: string;
  run_id: string;
  step_id: string;
  status: "pending" | "running" | "completed" | "failed";
  output: unknown;
  tokens_used: number | null;
  duration_ms: number | null;
  error: string | null;
  created_at: Date;
}

/** Matches sp_reports table (planned) */
export interface SpReport {
  id: string;
  run_id: string;
  title: string;
  content: string;
  format: "markdown" | "html" | "pdf";
  metadata: Record<string, unknown>;
  created_at: Date;
}

/** Matches sp_deliveries table — channel configuration per schedule */
export interface SpDelivery {
  id: string;
  schedule_id: string;
  channel: "email" | "webhook" | "telegram";
  config_json: Record<string, unknown>;
  enabled: boolean;
}
