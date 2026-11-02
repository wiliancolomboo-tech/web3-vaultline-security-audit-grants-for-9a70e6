# Copy and configure environment variables
cp .env.example .env
nano .env

# Launch Dockerized scanning agent and notification dispatcher
docker compose up -d --build

# Verify healthcheck endpoint
curl http://localhost:8080/health
# {"status":"UP","version":"1.0.0","solana_cluster":"devnet"}