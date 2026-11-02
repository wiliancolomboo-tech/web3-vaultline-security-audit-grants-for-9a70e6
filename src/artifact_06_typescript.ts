import axios from 'axios';
import { Resend } from 'resend';

export interface GrantNotificationPayload {
  eventType: 'INITIALIZED' | 'FUNDED' | 'ARTIFACT_SUBMITTED' | 'DISBURSED' | 'CANCELLED';
  projectId: string;
  grantPubkey: string;
  amountUSDC?: number;
  milestoneName?: string;
  artifactHash?: string;
  recipient?: string;
  txSignature?: string;
}

export class EnterpriseNotificationService {
  private resend: Resend | null = null;
  private slackWebhookUrl: string | undefined;
  private discordWebhookUrl: string | undefined;
  private alertEmailRecipient: string | undefined;

  constructor() {
    if (process.env.RESEND_API_KEY) {
      this.resend = new Resend(process.env.RESEND_API_KEY);
    }
    this.slackWebhookUrl = process.env.SLACK_WEBHOOK_URL;
    this.discordWebhookUrl = process.env.DISCORD_WEBHOOK_URL;
    this.alertEmailRecipient = process.env.NOTIFICATION_EMAIL || 'security-grants@vaultline.xyz';
  }

  public async broadcast(payload: GrantNotificationPayload): Promise<void> {
    const promises: Promise<any>[] = [];

    if (this.slackWebhookUrl) {
      promises.push(this.sendSlackAlert(payload));
    }
    if (this.discordWebhookUrl) {
      promises.push(this.sendDiscordAlert(payload));
    }
    if (this.resend && this.alertEmailRecipient) {
      promises.push(this.sendEmailAlert(payload));
    }

    const results = await Promise.allSettled(promises);
    for (const res of results) {
      if (res.status === 'rejected') {
        console.error('[NotificationService] Channel dispatch failed:', res.reason);
      }
    }
  }

  private async sendSlackAlert(payload: GrantNotificationPayload): Promise<void> {
    const text = `*Vaultline Grant Alert [${payload.eventType}]*\n` +
      `*Project:* ${payload.projectId}\n` +
      `*Grant PDA:* \`${payload.grantPubkey}\`\n` +
      (payload.amountUSDC ? `*Amount:* $${payload.amountUSDC.toLocaleString()} USDC\n` : '') +
      (payload.milestoneName ? `*Milestone:* ${payload.milestoneName}\n` : '') +
      (payload.artifactHash ? `*Audit Artifact Hash:* \`${payload.artifactHash}\`\n` : '') +
      (payload.txSignature ? `*Tx Signature:* <https://explorer.solana.com/tx/${payload.txSignature}?cluster=devnet|View on Solana Explorer>` : '');

    await axios.post(this.slackWebhookUrl!, { text });
  }

  private async sendDiscordAlert(payload: GrantNotificationPayload): Promise<void> {
    const embed = {
      title: `🛡️ Vaultline Colosseum Grant: ${payload.eventType}`,
      color: 0x00f0ff,
      fields: [
        { name: 'Colosseum Project ID', value: payload.projectId, inline: true },
        { name: 'Grant Escrow', value: `\`${payload.grantPubkey}\``, inline: false },
      ],
      footer: { text: 'Vaultline Security Verification Engine' },
      timestamp: new Date().toISOString(),
    };

    if (payload.amountUSDC) {
      embed.fields.push({ name: 'Disbursement', value: `$${payload.amountUSDC} USDC`, inline: true });
    }
    if (payload.milestoneName) {
      embed.fields.push({ name: 'Milestone', value: payload.milestoneName, inline: true });
    }

    await axios.post(this.discordWebhookUrl!, { embeds: [embed] });
  }

  private async sendEmailAlert(payload: GrantNotificationPayload): Promise<void> {
    if (!this.resend) return;

    await this.resend.emails.send({
      from: 'Vaultline Grants <grants@vaultline.xyz>',
      to: [this.alertEmailRecipient!],
      subject: `[Vaultline Audit Grant] Project ${payload.projectId} - ${payload.eventType}`,
      html: `
        <div style="font-family: Arial, sans-serif; line-height: 1.6; color: #111;">
          <h2 style="color: #0f172a;">Vaultline Colosseum Security Grant Update</h2>
          <p>An automated state transition was confirmed on the Solana Network.</p>
          <table style="width: 100%; border-collapse: collapse; margin-top: 12px;">
            <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Event</strong></td><td style="padding: 8px; border: 1px solid #ddd;">${payload.eventType}</td></tr>
            <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Project ID</strong></td><td style="padding: 8px; border: 1px solid #ddd;">${payload.projectId}</td></tr>
            <tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Escrow Address</strong></td><td style="padding: 8px; border: 1px solid #ddd;"><code>${payload.grantPubkey}</code></td></tr>
            ${payload.amountUSDC ? `<tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Amount</strong></td><td style="padding: 8px; border: 1px solid #ddd;">$${payload.amountUSDC} USDC</td></tr>` : ''}
            ${payload.milestoneName ? `<tr><td style="padding: 8px; border: 1px solid #ddd;"><strong>Milestone</strong></td><td style="padding: 8px; border: 1px solid #ddd;">${payload.milestoneName}</td></tr>` : ''}
          </table>
          <p style="margin-top: 16px;">Verified by <strong>Vaultline Security Audit Protocol</strong>.</p>
        </div>
      `,
    });
  }
}