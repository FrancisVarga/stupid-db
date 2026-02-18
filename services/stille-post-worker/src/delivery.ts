import { createTransport, type Transporter } from "nodemailer";
import type postgres from "postgres";
import type { Config } from "./config.js";
import { getDb } from "./db.js";
import type { GeneratedReport } from "./report-generator.js";

// ---------------------------------------------------------------------------
// Channel config types — stored as JSONB in sp_deliveries.config_json
// ---------------------------------------------------------------------------

export interface EmailConfig {
  host: string;
  port: number;
  secure: boolean;
  auth: { user: string; pass: string };
  from: string;
  to: string[];
}

export interface WebhookConfig {
  url: string;
  method?: string;
  headers?: Record<string, string>;
}

export interface TelegramConfig {
  botToken: string;
  chatId: string;
}

type ChannelConfig = EmailConfig | WebhookConfig | TelegramConfig;

// ---------------------------------------------------------------------------
// Result type
// ---------------------------------------------------------------------------

export interface DeliveryResult {
  deliveryId: string;
  channel: string;
  success: boolean;
  error?: string;
  sentAt: Date;
}

// ---------------------------------------------------------------------------
// Delivery engine
// ---------------------------------------------------------------------------

export class DeliveryEngine {
  private config: Config;

  constructor(config: Config) {
    this.config = config;
  }

  /**
   * Deliver a report to all enabled delivery channels for a given schedule.
   *
   * Queries sp_deliveries for the schedule, then dispatches to each channel.
   * Failures are captured per-channel — one channel failing doesn't block others.
   */
  async deliverReport(
    report: GeneratedReport,
    pipelineName: string,
    scheduleId: string,
  ): Promise<DeliveryResult[]> {
    const sql = getDb(this.config);

    // Load delivery channels for this schedule
    const deliveries = await sql`
      SELECT id, channel, config_json
      FROM sp_deliveries
      WHERE schedule_id = ${scheduleId}
        AND enabled = true
    `;

    if (deliveries.length === 0) {
      console.log(`[Delivery] No delivery channels configured for schedule ${scheduleId}`);
      return [];
    }

    // Dispatch to all channels in parallel — isolated error handling per channel
    const results = await Promise.all(
      deliveries.map((d) =>
        this.dispatchToChannel(
          d.id,
          d.channel as "email" | "webhook" | "telegram",
          d.config_json as ChannelConfig,
          report,
          pipelineName,
        ),
      ),
    );

    console.log(
      `[Delivery] ${results.filter((r) => r.success).length}/${results.length} deliveries succeeded for report ${report.id}`,
    );

    return results;
  }

  // -------------------------------------------------------------------------
  // Channel dispatch
  // -------------------------------------------------------------------------

  private async dispatchToChannel(
    deliveryId: string,
    channel: "email" | "webhook" | "telegram",
    channelConfig: ChannelConfig,
    report: GeneratedReport,
    pipelineName: string,
  ): Promise<DeliveryResult> {
    const sentAt = new Date();

    try {
      switch (channel) {
        case "email":
          await this.sendEmail(channelConfig as EmailConfig, report, pipelineName);
          break;
        case "webhook":
          await this.sendWebhook(channelConfig as WebhookConfig, report, pipelineName);
          break;
        case "telegram":
          await this.sendTelegram(channelConfig as TelegramConfig, report);
          break;
      }

      console.log(`[Delivery] ${channel} sent for delivery ${deliveryId}`);
      return { deliveryId, channel, success: true, sentAt };
    } catch (err) {
      const error = err instanceof Error ? err.message : String(err);
      console.error(`[Delivery] ${channel} failed for delivery ${deliveryId}:`, error);
      return { deliveryId, channel, success: false, error, sentAt };
    }
  }

  // -------------------------------------------------------------------------
  // Email via nodemailer
  // -------------------------------------------------------------------------

  private async sendEmail(
    config: EmailConfig,
    report: GeneratedReport,
    pipelineName: string,
  ): Promise<void> {
    const transport: Transporter = createTransport({
      host: config.host,
      port: config.port,
      secure: config.secure,
      auth: config.auth,
    });

    const date = new Date().toLocaleDateString("en-US", { dateStyle: "medium" });
    const subject = `[Stille Post] ${pipelineName} - ${date}`;

    await transport.sendMail({
      from: config.from,
      to: config.to.join(", "),
      subject,
      html: report.contentHtml,
    });
  }

  // -------------------------------------------------------------------------
  // Webhook via native fetch
  // -------------------------------------------------------------------------

  private async sendWebhook(
    config: WebhookConfig,
    report: GeneratedReport,
    pipelineName: string,
  ): Promise<void> {
    const method = config.method ?? "POST";
    const headers: Record<string, string> = {
      "Content-Type": "application/json",
      ...config.headers,
    };

    const body = JSON.stringify({
      pipeline: pipelineName,
      reportId: report.id,
      title: report.title,
      content: report.contentJson,
      renderBlocks: report.renderBlocks,
      sentAt: new Date().toISOString(),
    });

    const resp = await fetch(config.url, { method, headers, body });

    if (!resp.ok) {
      throw new Error(`Webhook returned ${resp.status}: ${await resp.text()}`);
    }
  }

  // -------------------------------------------------------------------------
  // Telegram via Bot API
  // -------------------------------------------------------------------------

  private async sendTelegram(
    config: TelegramConfig,
    report: GeneratedReport,
  ): Promise<void> {
    // Telegram messages max out at 4096 chars — send a text summary
    const summary = buildTelegramSummary(report);

    const url = `https://api.telegram.org/bot${config.botToken}/sendMessage`;
    const resp = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({
        chat_id: config.chatId,
        text: summary,
        parse_mode: "HTML",
      }),
    });

    if (!resp.ok) {
      const body = await resp.text();
      throw new Error(`Telegram API returned ${resp.status}: ${body}`);
    }
  }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Build a Telegram-friendly text summary from a report.
 * Strips HTML tags and truncates to 4096 chars (Telegram limit).
 */
function buildTelegramSummary(report: GeneratedReport): string {
  const header = `<b>${escapeHtml(report.title)}</b>\n\n`;

  // Use the contentJson sections for a clean text summary
  const sections =
    (report.contentJson.sections as Array<{ agent: string; content: string }>) ?? [];

  let body = sections
    .map((s) => `<b>${escapeHtml(s.agent)}</b>\n${escapeHtml(s.content)}`)
    .join("\n\n");

  // Truncate to fit Telegram's 4096-char limit (accounting for header)
  const maxLen = 4096 - header.length - 3; // 3 for "..."
  if (body.length > maxLen) {
    body = body.slice(0, maxLen) + "...";
  }

  return header + body;
}

function escapeHtml(str: string): string {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;");
}
