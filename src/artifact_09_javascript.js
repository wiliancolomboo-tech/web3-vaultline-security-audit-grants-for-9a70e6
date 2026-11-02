const express = require("express");
const app = express();
const port = process.env.PORT || 8080;

app.use(express.json());

app.get("/healthz", (req, res) => {
  res.status(200).json({
    status: "healthy",
    subsystem: "vaultline-audit-grants-daemon",
    timestamp: new Date().toISOString(),
    network: process.env.SOLANA_RPC_URL || "devnet",
  });
});

app.listen(port, () => {
  console.log(`[🚀] Vaultline Telemetry server online on port ${port}`);
});