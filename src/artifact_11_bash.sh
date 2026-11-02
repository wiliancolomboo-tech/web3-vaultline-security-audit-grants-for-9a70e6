# 1. Build Solana Program
anchor build

# 2. Deploy Program to Solana Devnet / Mainnet
solana program deploy \
  --program-id target/deploy/vaultline_sentinel-keypair.json \
  target/deploy/vaultline_sentinel.so \
  --url https://api.devnet.solana.com

# 3. Launch the Turnkey Worker & Telemetry Container
docker-compose up -d --build

# 4. Verify Prometheus / Health Check API
curl -s http://localhost:8080/health | jq .