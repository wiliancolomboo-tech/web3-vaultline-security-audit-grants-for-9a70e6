import * as anchor from '@coral-xyz/anchor';
import { Connection, PublicKey } from '@solana/web3.js';
import * as dotenv from 'dotenv';
import { EnterpriseNotificationService } from './notifier';

dotenv.config();

const RPC_URL = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const PROGRAM_ID = new PublicKey(process.env.PROGRAM_ID || 'VLinEK3yZ5Xg8V9Pj1R7u1z5iY7L8sZ8xQGv9aK8oJ2');
const notifier = new EnterpriseNotificationService();

async function startTelemetryDaemon() {
  console.log(`=======================================================`);
  console.log(` VAULTLINE GRANT TELEMETRY DAEMON: RUNNING `);
  console.log(` Tracking Program: ${PROGRAM_ID.toBase58()}`);
  console.log(` Connection: ${RPC_URL}`);
  console.log(`=======================================================`);

  const connection = new Connection(RPC_URL, {
    commitment: 'confirmed',
    wsEndpoint: RPC_URL.replace('http', 'ws'),
  });

  connection.onLogs(
    PROGRAM_ID,
    async (logs, ctx) => {
      if (logs.err) return;
      const signature = logs.signature;
      console.log(`[Telemetry Event] Detected execution: ${signature}`);

      for (const logLine of logs.logs) {
        if (logLine.includes('GrantInitializedEvent')) {
          console.log(`>>> Event: Grant Initialized`);
        } else if (logLine.includes('EscrowFundedEvent')) {
          console.log(`>>> Event: Escrow Funded`);
        } else if (logLine.includes('ArtifactSubmittedEvent')) {
          console.log(`>>> Event: Artifact Submitted`);
        } else if (logLine.includes('MilestoneDisbursedEvent')) {
          console.log(`>>> Event: Tranche Disbursed`);
        }
      }
    },
    'confirmed'
  );
}

startTelemetryDaemon().catch((err) => {
  console.error('[Telemetry Fatal Error]', err);
  process.exit(1);
});