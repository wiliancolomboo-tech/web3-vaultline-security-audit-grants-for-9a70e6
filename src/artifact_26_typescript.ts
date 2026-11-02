// Telemetry & Healthcheck express endpoint embedded in worker
import * as http from 'http';

const server = http.createServer((req, res) => {
  if (req.url === '/health' && req.method === 'GET') {
    res.writeHead(200, { 'Content-Type': 'application/json' });
    res.end(JSON.stringify({
      status: 'UP',
      service: 'vaultline-audit-oracle',
      cluster: process.env.SOLANA_RPC_URL || 'devnet',
      timestamp: Date.now()
    }));
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(process.env.PORT || 8080);