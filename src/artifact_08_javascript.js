const { Connection, PublicKey } = require("@solana/web3.js");
require("dotenv").config();

const RPC_URL = process.env.SOLANA_RPC_URL || "https://api.devnet.solana.com";
const PROGRAM_ID = new PublicKey(process.env.PROGRAM_ID || "VLinEUh5h6UkmV4Q285h2c5bC6u8VwZ3Nf4xN89e17b");

async function startDaemon() {
  console.log(`[DAEMON] Starting Vaultline GrantShield Event Worker...`);
  console.log(`[DAEMON] Monitoring Program: ${PROGRAM_ID.toBase58()} on ${RPC_URL}`);

  const connection = new Connection(RPC_URL, {
    commitment: "confirmed",
    wsEndpoint: RPC_URL.replace("https://", "wss://").replace("http://", "ws://"),
  });

  connection.onLogs(
    PROGRAM_ID,
    (logs, ctx) => {
      console.log(`[LOG RECVD] Slot: ${ctx.slot}, Signature: ${logs.signature}`);
      for (const log of logs.logs) {
        if (log.includes("GrantInitialized")) {
          console.log(`>>> Event: New Colosseum Audit Grant Initialized!`);
        } else if (log.includes("AuditorAttestationSubmitted")) {
          console.log(`>>> Event: Security Auditor Submitted Cryptographic Attestation!`);
        } else if (log.includes("GrantDisbursed")) {
          console.log(`>>> Event: Dual-Key Verified. USDC Disbursed!`);
        }
      }
    },
    "confirmed"
  );
}

startDaemon().catch((err) => {
  console.error(`[FATAL] Daemon failed:`, err);
  process.exit(1);
});