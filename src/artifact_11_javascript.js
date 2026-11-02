const http = require("http");

const PORT = process.env.PORT || 8080;
const server = http.createServer((req, res) => {
  if (req.url === "/health" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({
      status: "healthy",
      service: "vaultline-grant-protocol-daemon",
      network: "solana-devnet",
      timestamp: new Date().toISOString()
    }));
  } else {
    res.writeHead(404);
    res.end();
  }
});

server.listen(PORT, () => {
  console.log(`[Telemetry] Healthcheck server live on port ${PORT}`);
});