import express from "express";
import { Connection } from "@solana/web3.js";

const app = express();
const port = process.env.PORT || 8080;
const connection = new Connection(process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com");

app.get("/health", async (req, res) => {
  try {
    const slot = await connection.getSlot();
    const version = await connection.getVersion();
    res.status(200).json({
      status: "HEALTHY",
      solanaSlot: slot,
      nodeVersion: version["solana-core"],
      timestamp: new Date().toISOString(),
    });
  } catch (err: any) {
    res.status(503).json({
      status: "DEGRADED",
      error: err.message,
    });
  }
});

app.listen(port, () => {
  console.log(`Vaultline Grant Telemetry Server listening on port ${port}`);
});