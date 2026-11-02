import axios from 'axios';

export interface AuditAlertPayload {
  projectSlug: string;
  grantPubkey: string;
  criticals: number;
  highs: number;
  mediums: number;
  lows: number;
  reportHashHex: string;
}

export class SlackNotificationService {
  private webhookUrl: string | undefined;

  constructor() {
    this.webhookUrl = process.env.SLACK_WEBHOOK_URL;
  }

  async sendAuditReportAlert(payload: AuditAlertPayload): Promise<void> {
    if (!this.webhookUrl) {
      console.log(`[DRY-RUN SLACK] Audit Report Notarized for ${payload.projectSlug}: Criticals=${payload.criticals}, Highs=${payload.highs}`);
      return;
    }

    const message = {
      text: `🚨 *Vaultline Audit Report Notarized: ${payload.projectSlug}*`,
      blocks: [
        {
          type: "header",
          text: {
            type: "plain_text",
            text: `🛡️ Audit Report Lodged: ${payload.projectSlug}`
          }
        },
        {
          type: "section",
          fields: [
            { type: "mrkdwn", text: `*Grant Account:*\n\`${payload.grantPubkey}\`` },
            { type: "mrkdwn", text: `*Report SHA-256:*\n\`${payload.reportHashHex.slice(0, 16)}...\`` },
            { type: "mrkdwn", text: `*Critical Severity:*\n🔥 ${payload.criticals}` },
            { type: "mrkdwn", text: `*High Severity:*\n⚠️ ${payload.highs}` }
          ]
        },
        {
          type: "context",
          elements: [
            {
              type: "mrkdwn",
              text: `Audit Grant Verified via Vaultline Solana Engine | Status: Ready for Milestone Release`
            }
          ]
        }
      ]
    };

    await axios.post(this.webhookUrl, message, {
      headers: { 'Content-Type': 'application/json' }
    });
  }
}