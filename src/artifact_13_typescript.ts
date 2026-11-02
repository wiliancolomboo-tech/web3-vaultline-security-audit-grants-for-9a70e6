import http from "http";

export function startHealthServer(port: number, client: any) {
  const server = http.createServer(async (req, res) => {
    if (req.url === "/health" && req.method === "GET") {
      try {
        const slot = await client.connection.getSlot();
        res.writeHead(200, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "healthy", currentSlot: slot, timestamp: Date.now() }));
      } catch (err: any) {
        res.writeHead(503, { "Content-Type": "application/json" });
        res.end(JSON.stringify({ status: "unhealthy", error: err.message }));
      }
    } else {
      res.writeHead(404);
      res.end();
    }
  });

  server.listen(port, () => {
    console.log(`[HealthCheck] Listening on port ${port}`);
  });
}