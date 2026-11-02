import { Connection, PublicKey, Keypair } from "@solana/web3.js";
import { Program, AnchorProvider, Wallet } from "@coral-xyz/anchor";
import { Resend } from "resend";
import axios from "axios";
import * as crypto from "crypto";

export interface SentinelConfig {
  rpcEndpoint: string;
  vaultlineProgramId: string;
  resendApiKey: string;
  notificationEmail: string;
  slackWebhookUrl: string;
  adminKeypair: Keypair;
}

export class VaultlineSentinel {
  private connection: Connection;
  private resend: Resend;
  private slackWebhookUrl: string;
  private notificationEmail: string;

  constructor(private config: SentinelConfig) {
    this.connection = new Connection(config.rpcEndpoint, "confirmed");
    this.resend = new Resend(config.resendApiKey);
    this.slackWebhookUrl = config.slackWebhookUrl;
    this.notificationEmail = config.notificationEmail;
  }

  /**
   * Dispatches dual alerting: transactional email via Resend and real-time incident alert to Slack/Discord.
   */
  public async broadcastAlert(event: string, details: Record<string, any>): Promise<void> {
    const formattedDate = new Date().toISOString();
    console.log(`[ALERT] [${formattedDate}] ${event}:`, details);

    // 1. Resend Transactional Email
    try {
      await this.resend.emails.send({
        from: "Vaultline Security Grants <alerts@vaultline.security>",
        to: [this.notificationEmail],
        subject: `[Vaultline Audit Grant] ${event} - Project ${details.projectId}`,
        html: `
          <div style="font-family: Arial, sans-serif; background: #0f172a; color: #f8fafc; padding: 24px; border-radius: 8px;">
            <h2 style="color: #38bdf8;">Vaultline Security Grant Update</h2>
            <p><strong>Event:</strong> ${event}</p>
            <p><strong>Project ID:</strong> ${details.projectId}</p>
            <p><strong>Transaction Signature:</strong> <code>${details.signature || "N/A"}</code></p>
            <hr style="border: 1px solid #334155;" />
            <pre style="background: #1e293b; padding: 12px; border-radius: 6px; overflow-x: auto;">
${JSON.stringify(details, null, 2)}
            </pre>
            <p style="font-size: 12px; color: #94a3b8;">Deterministic On-Chain Attestation verified on Solana Mainnet.</p>
          </div>
        `,
      });
    } catch (err: any) {
      console.error("[RESEND_ERROR] Failed to send email alert:", err.message);
    }

    // 2. Slack Enterprise Webhook
    try {
      if (this.slackWebhookUrl) {
        await axios.post(this.slackWebhookUrl, {
          text: `🛡️ *Vaultline Security Grant Notification*: \`${event}\` for Project \`${details.projectId}\``,
          blocks: [
            {
              type: "header",
              text: {
                type: "plain_text",
                text: `🛡️ Vaultline Grant: ${event}`,
                emoji: true,
              },
            },
            {
              type: "section",
              fields: [
                { type: "mrkdwn", text: `*Project:*\n${details.projectId}` },
                { type: "mrkdwn", text: `*Amount:*\n${details.amount || "N/A"} USDC` },
                { type: "mrkdwn", text: `*Auditor:*\n\`${details.auditor || "N/A"}\`` },
                { type: "mrkdwn", text: `*Program ID:*\n\`${details.targetProgramId || "N/A"}\`` },
              ],
            },
          ],
        });
      }
    } catch (err: any) {
      console.error("[SLACK_ERROR] Failed to push webhook:", err.message);
    }
  }

  /**
   * Helper utility to calculate git commit SHA and audit report SHA-256 for deterministic on-chain attestation.
   */
  public static computeReportHash(reportBuffer: Buffer): Buffer {
    return crypto.createHash("sha256").update(reportBuffer).digest();
  }

  public static hexToCommitBytes(hexSha: string): number[] {
    const clean = hexSha.replace(/^0x/, "");
    if (clean.length !== 40) throw new Error("Invalid git commit SHA-1 length");
    const arr = [];
    for (let i = 0; i < 40; i += 2) {
      arr.push(parseInt(clean.substr(i, 2), 16));
    }
    return arr;
  }
}