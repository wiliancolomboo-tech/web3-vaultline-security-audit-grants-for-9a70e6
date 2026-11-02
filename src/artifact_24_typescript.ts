import http from 'http';
import pino from 'pino';
import { Connection } from '@solana/web3.js';

const logger = pino({ name: 'VaultlineWorkerService' });
const PORT = process.env.PORT || 8080;
const rpcUrl = process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com';
const connection = new Connection(rpcUrl, 'confirmed');

const server = http.createServer(async (req, res) => {
  if (req.url === '/health' && req.method === 'GET') {
    try {
      const slot = await connection.getSlot();
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: 'HEALTHY', latestSlot: slot, timestamp: new Date().toISOString() }));
    } catch (err: any) {
      res.writeHead(503, { 'Content-Type': 'application/json' });
      res.end(JSON.stringify({ status: 'DEGRADED', error: err.message }));
    }
  } else {
    res.writeHead(404);
    res.end();
  }
});

server.listen(PORT, () => {
  logger.info(`Vaultline SGE Worker live on port ${PORT}`);
});