import cron from "node-cron";
import cronParser from "cron-parser";
import type { Config } from "./config.js";
import { getDb } from "./db.js";
import { executePipeline } from "./orchestrator.js";
import { generateReport } from "./report-generator.js";
import { DeliveryEngine } from "./delivery.js";

export class SchedulePoller {
  private task: cron.ScheduledTask | null = null;
  private running = false;
  private config: Config;
  private delivery: DeliveryEngine;

  constructor(config: Config) {
    this.config = config;
    this.delivery = new DeliveryEngine(config);
  }

  /** Start polling every minute for due schedules. */
  start() {
    this.task = cron.schedule("* * * * *", () => this.poll());
    console.log("[Scheduler] Polling started (every 60s)");
  }

  stop() {
    this.task?.stop();
    console.log("[Scheduler] Polling stopped");
  }

  private async poll() {
    if (this.running) return; // prevent overlapping polls
    this.running = true;
    try {
      const sql = getDb(this.config);

      // Find schedules that are due
      const due = await sql`
        SELECT * FROM sp_schedules
        WHERE enabled = true
          AND next_run_at <= now()
        ORDER BY next_run_at ASC
        LIMIT 10
      `;

      for (const schedule of due) {
        await this.executeSchedule(schedule);
      }
    } catch (err) {
      console.error("[Scheduler] Poll error:", err);
    } finally {
      this.running = false;
    }
  }

  private async executeSchedule(schedule: any) {
    const sql = getDb(this.config);
    try {
      // Calculate next run time and update the schedule
      const next = this.calculateNextRun(
        schedule.cron_expression,
        schedule.timezone,
      );
      await sql`
        UPDATE sp_schedules
        SET last_run_at = now(), next_run_at = ${next}
        WHERE id = ${schedule.id}
      `;

      // Trigger pipeline execution
      const result = await executePipeline(
        schedule.pipeline_id,
        "scheduled",
        this.config,
        schedule.id,
      );

      // Generate report and deliver if pipeline succeeded
      if (result.status === "completed") {
        const pipelineName =
          (await sql`SELECT name FROM sp_pipelines WHERE id = ${schedule.pipeline_id}`)[0]
            ?.name ?? "Unknown Pipeline";

        const agentOutputs = result.steps.map((s, i) => ({
          agentName: s.agentId,
          output: s.output,
        }));

        const report = await generateReport(sql, result.runId, pipelineName, agentOutputs);
        await this.delivery.deliverReport(report, pipelineName, schedule.id);
      }
    } catch (err) {
      console.error(`[Scheduler] Failed schedule ${schedule.id}:`, err);
    }
  }

  private calculateNextRun(cronExpr: string, timezone: string): Date {
    const interval = cronParser.parseExpression(cronExpr, {
      tz: timezone || "UTC",
    });
    return interval.next().toDate();
  }
}
