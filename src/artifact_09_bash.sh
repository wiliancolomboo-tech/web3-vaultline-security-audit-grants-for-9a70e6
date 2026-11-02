# 1. Build Solana Anchor Program
anchor build

# 2. Deploy to Solana Devnet or Mainnet
solana program deploy \
  --program-id target/deploy/vaultline_core-keypair.json \
  target/deploy/vaultline_core.so \
  --url devnet

# 3. Run full automated integration test suite
anchor test

# 4. Launch Turnkey Dockerized Sentinel Worker
docker-compose up -d --build