import http from "http";
import { VaultlineShieldAgent } from "./agent";

const PORT = process.env.PORT || 8080;

const agent = new VaultlineShieldAgent({
  rpcUrl: process.env.RPC_URL || "http://127.0.0.1:8899",
  programId: process.env.PROGRAM_ID || "V1tSh1e1d1111111111111111111111111111111111",
  discordWebhookUrl: process.env.DISCORD_WEBHOOK_URL,
  slackWebhookUrl: process.env.SLACK_WEBHOOK_URL,
  resendApiKey: process.env.RESEND_API_KEY,
  notificationEmail: process.env.AUDIT_ALERT_EMAIL,
});

agent.startEventListener();

const server = http.createServer((req, res) => {
  if (req.url === "/health") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "healthy", timestamp: Date.now() }));
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(PORT, () => {
  console.log(`[HealthServer] Healthcheck listening on port ${PORT}`);
});