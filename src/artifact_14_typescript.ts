import * as http from "http";

const PORT = process.env.PORT || 8080;
const server = http.createServer((req, res) => {
  if (req.url === "/healthz") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(JSON.stringify({ status: "healthy", timestamp: Date.now(), service: "vaultline-grant-shield" }));
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(PORT, () => {
  console.log(`[TELEMETRY] Healthcheck running on port ${PORT}`);
});