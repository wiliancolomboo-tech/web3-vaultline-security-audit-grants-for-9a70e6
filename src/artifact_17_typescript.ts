import axios from "axios";
import { Resend } from "resend";

export interface GrantNotificationPayload {
  grantId: string;
  projectName: string;
  auditorAddress: string;
  amountUsdc: number;
  stage: "FUNDED" | "AUDIT_SUBMITTED" | "MILESTONE_1_PAID" | "COMPLETED" | "CLAWBACK";
  reportHash?: string;
  explorerUrl: string;
}

export class EnterpriseNotifier {
  private resendClient?: Resend;
  private webhookUrl?: string;

  constructor() {
    const resendKey = process.env.RESEND_API_KEY;
    if (resendKey) {
      this.resendClient = new Resend(resendKey);
    }
    this.webhookUrl = process.env.SECOPS_WEBHOOK_URL;
  }

  public async broadcast(payload: GrantNotificationPayload): Promise<void> {
    const timestamp = new Date().toISOString();
    console.log(`[Notifier] Broadcasting event: ${payload.stage} for Grant ${payload.grantId}`);

    // 1. Dispatch Webhook (Slack / Discord Enterprise channel)
    if (this.webhookUrl) {
      const message = {
        text: `🛡️ *Vaultline Security Grant Event: ${payload.stage}*`,
        attachments: [
          {
            color: payload.stage === "COMPLETED" ? "#36a64f" : "#205493",
            fields: [
              { title: "Grant ID", value: payload.grantId, short: true },
              { title: "Project", value: payload.projectName, short: true },
              { title: "Amount (USDC)", value: `$${payload.amountUsdc.toLocaleString()}`, short: true },
              { title: "Auditor", value: payload.auditorAddress, short: true },
              { title: "Report Digest", value: payload.reportHash || "N/A", short: false },
              { title: "Solana Explorer", value: `<${payload.explorerUrl}|View Transaction>`, short: false },
              { title: "Timestamp", value: timestamp, short: false },
            ],
          },
        ],
      };

      try {
        await axios.post(this.webhookUrl, message, { timeout: 5000 });
        console.log("[Notifier] Webhook dispathed successfully.");
      } catch (err: any) {
        console.error("[Notifier] Failed to deliver webhook:", err.message);
      }
    }

    // 2. Dispatch Transactional Audit Certificate via Resend
    if (this.resendClient && process.env.ALERT_EMAIL_RECIPIENT) {
      try {
        await this.resendClient.emails.send({
          from: "Vaultline Security Grants <grants@vaultline.io>",
          to: [process.env.ALERT_EMAIL_RECIPIENT],
          subject: `[Vaultline Audit Notification] ${payload.projectName}: ${payload.stage}`,
          html: `
            <h2>Vaultline Security Audit Grants Protocol</h2>
            <p><strong>Stage:</strong> ${payload.stage}</p>
            <p><strong>Grant ID:</strong> ${payload.grantId}</p>
            <p><strong>Project Name:</strong> ${payload.projectName}</p>
            <p><strong>Grant Value:</strong> ${payload.amountUsdc} USDC</p>
            <p><strong>Auditor Authority:</strong> <code>${payload.auditorAddress}</code></p>
            ${payload.reportHash ? `<p><strong>Cryptographic Digest (SHA-256):</strong> <code>${payload.reportHash}</code></p>` : ""}
            <p><a href="${payload.explorerUrl}">View State Verification On Solana Explorer</a></p>
            <hr/>
            <small>Immutable confirmation secured via Anchor Solana PDA Escrow.</small>
          `,
        });
        console.log("[Notifier] Resend email dispatched to recipient.");
      } catch (err: any) {
        console.error("[Notifier] Failed to send email via Resend:", err.message);
      }
    }
  }
}