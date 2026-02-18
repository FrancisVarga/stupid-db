import type { Config } from "./config.js";
import { getDb } from "./db.js";
import { runAgent } from "./agent-runner.js";
import type { SpPipelineStep } from "./types.js";

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

export interface PipelineResult {
  runId: string;
  status: "completed" | "failed";
  steps: StepResult[];
  finalOutput: string;
  totalTokens: number;
  totalDurationMs: number;
}

export interface StepResult {
  stepId: string;
  agentId: string;
  status: "completed" | "failed";
  output: string;
  tokensUsed: number;
  durationMs: number;
  error?: string;
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/** Group steps by parallel_group, preserving step_order. */
function groupSteps(steps: SpPipelineStep[]): SpPipelineStep[][] {
  const groups: SpPipelineStep[][] = [];
  let currentGroup: SpPipelineStep[] = [];
  let currentKey: number | null | "INIT" = "INIT";

  for (const step of steps) {
    const key = step.parallel_group;

    if (key === null) {
      // Sequential step — flush any accumulated parallel group, then push solo
      if (currentGroup.length > 0) {
        groups.push(currentGroup);
        currentGroup = [];
      }
      groups.push([step]);
      currentKey = "INIT";
    } else if (key === currentKey) {
      // Same parallel group — accumulate
      currentGroup.push(step);
    } else {
      // New parallel group — flush previous, start new
      if (currentGroup.length > 0) {
        groups.push(currentGroup);
      }
      currentGroup = [step];
      currentKey = key;
    }
  }

  if (currentGroup.length > 0) {
    groups.push(currentGroup);
  }

  return groups;
}

/** Combine outputs from a parallel group into a single string. */
function mergeGroupOutputs(results: StepResult[]): string {
  if (results.length === 1) return results[0].output;
  return results
    .map((r, i) => `--- Agent ${i + 1} (${r.agentId}) ---\n${r.output}`)
    .join("\n\n");
}

// ---------------------------------------------------------------------------
// Core orchestrator
// ---------------------------------------------------------------------------

export async function executePipeline(
  pipelineId: string,
  triggerType: "manual" | "scheduled" | "event",
  config: Config,
  scheduleId?: string,
  initialInput?: Record<string, unknown>,
): Promise<PipelineResult> {
  const sql = getDb(config);
  const overallStart = Date.now();

  // 1. Create run record
  const [run] = await sql`
    INSERT INTO sp_runs (pipeline_id, schedule_id, status, trigger_type, started_at)
    VALUES (${pipelineId}, ${scheduleId ?? null}, 'running', ${triggerType}, now())
    RETURNING id
  `;
  const runId: string = run.id;

  const allStepResults: StepResult[] = [];

  try {
    // 2. Load pipeline steps ordered by step_order
    const steps = await sql<SpPipelineStep[]>`
      SELECT id, pipeline_id, agent_id, step_order, input_mapping,
             output_mapping, parallel_group, data_source_id
      FROM sp_pipeline_steps
      WHERE pipeline_id = ${pipelineId}
      ORDER BY step_order ASC, parallel_group ASC NULLS FIRST
    `;

    if (steps.length === 0) {
      throw new Error(`Pipeline ${pipelineId} has no steps`);
    }

    // 3. Group by parallel_group
    const groups = groupSteps(steps as SpPipelineStep[]);

    // 4. Execute groups sequentially; agents within a group in parallel
    let previousOutput = initialInput
      ? JSON.stringify(initialInput)
      : "";

    for (const group of groups) {
      const groupResults = await Promise.all(
        group.map((step) => executeStep(step, previousOutput, runId, config)),
      );

      allStepResults.push(...groupResults);

      // If any step in the group failed, abort the pipeline
      const failed = groupResults.find((r) => r.status === "failed");
      if (failed) {
        throw new Error(
          `Step ${failed.stepId} (agent ${failed.agentId}) failed: ${failed.error}`,
        );
      }

      // 5. Pass merged output to next group
      previousOutput = mergeGroupOutputs(groupResults);
    }

    // 6. Mark run as completed
    await sql`
      UPDATE sp_runs
      SET status = 'completed', completed_at = now()
      WHERE id = ${runId}
    `;

    const totalDurationMs = Date.now() - overallStart;
    const totalTokens = allStepResults.reduce((s, r) => s + r.tokensUsed, 0);

    return {
      runId,
      status: "completed",
      steps: allStepResults,
      finalOutput: previousOutput,
      totalTokens,
      totalDurationMs,
    };
  } catch (err) {
    // 6b. Mark run as failed
    const errorMsg = err instanceof Error ? err.message : String(err);
    await sql`
      UPDATE sp_runs
      SET status = 'failed', completed_at = now(), error = ${errorMsg}
      WHERE id = ${runId}
    `;

    const totalDurationMs = Date.now() - overallStart;
    const totalTokens = allStepResults.reduce((s, r) => s + r.tokensUsed, 0);

    return {
      runId,
      status: "failed",
      steps: allStepResults,
      finalOutput: "",
      totalTokens,
      totalDurationMs,
    };
  }
}

// ---------------------------------------------------------------------------
// Single step execution
// ---------------------------------------------------------------------------

async function executeStep(
  step: SpPipelineStep,
  input: string,
  runId: string,
  config: Config,
): Promise<StepResult> {
  const sql = getDb(config);
  const agentId = step.agent_id ?? "unknown";
  const stepStart = Date.now();

  // Create step_result record
  const [resultRow] = await sql`
    INSERT INTO sp_step_results (run_id, step_id, agent_id, input_data, status)
    VALUES (${runId}, ${step.id}, ${agentId}, ${JSON.stringify(input)}, 'running')
    RETURNING id
  `;

  try {
    const output = await runAgent(agentId, input, config);
    const durationMs = Date.now() - stepStart;

    // Update step result — tokens_used will come from the real agent runner
    await sql`
      UPDATE sp_step_results
      SET status = 'completed',
          output_data = ${JSON.stringify(output)},
          duration_ms = ${durationMs},
          tokens_used = 0
      WHERE id = ${resultRow.id}
    `;

    return {
      stepId: step.id,
      agentId,
      status: "completed",
      output,
      tokensUsed: 0,
      durationMs,
    };
  } catch (err) {
    const durationMs = Date.now() - stepStart;
    const errorMsg = err instanceof Error ? err.message : String(err);

    await sql`
      UPDATE sp_step_results
      SET status = 'failed',
          duration_ms = ${durationMs},
          tokens_used = 0
      WHERE id = ${resultRow.id}
    `;

    return {
      stepId: step.id,
      agentId,
      status: "failed",
      output: "",
      tokensUsed: 0,
      durationMs,
      error: errorMsg,
    };
  }
}
