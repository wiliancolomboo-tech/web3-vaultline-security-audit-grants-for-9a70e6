# 1. Build and Test Solana Anchor Program
anchor build
anchor test

# 2. Deploy to Solana Devnet / Mainnet
solana config set --url https://api.devnet.solana.com
anchor deploy --program-name vaultline_grants --program-keypair ./target/deploy/vaultline_grants-keypair.json

# 3. Initialize Master Grant Pool with USDC Mint
ts-node client/src/cli.ts init-pool --mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v

# 4. Launch Automation & Webhook Orchestrator
docker-compose up -d --build