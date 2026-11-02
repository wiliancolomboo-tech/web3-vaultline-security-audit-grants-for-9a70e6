const http = require("http");
const port = process.env.PORT || 8080;

const server = http.createServer((req, res) => {
  if (req.url === "/health" && req.method === "GET") {
    res.writeHead(200, { "Content-Type": "application/json" });
    return res.end(
      JSON.stringify({
        status: "HEALTHY",
        uptime: process.uptime(),
        timestamp: new Date().toISOString(),
        protocol: "Vaultline Colosseum Grants v1.0",
      })
    );
  }
  res.writeHead(404);
  res.end();
});

server.listen(port, () => {
  console.log(`Vaultline Grant Daemon health server active on port ${port}`);
});