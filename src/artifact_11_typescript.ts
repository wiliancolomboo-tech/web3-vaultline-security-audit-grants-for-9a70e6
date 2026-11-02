import http from "http";

const PORT = process.env.PORT || 8080;

const server = http.createServer((req, res) => {
  if (req.url === "/health" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(
      JSON.stringify({
        status: "HEALTHY",
        uptimeSeconds: process.uptime(),
        timestamp: new Date().toISOString(),
        service: "vaultline-audit-sentinel",
      })
    );
    return;
  }
  res.writeHead(404);
  res.end();
});

server.listen(PORT, () => {
  console.log(`[VAULTLINE-HEALTH] Sentinel daemon healthcheck listening on port ${PORT}`);
});