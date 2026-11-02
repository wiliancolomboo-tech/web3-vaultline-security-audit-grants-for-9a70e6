# 1. Clone repository and install dependencies
git clone https://github.com/vaultline-security/vaultline-guard.git
cd vaultline-guard
npm install

# 2. Build Anchor Smart Contract
anchor build

# 3. Deploy to Solana Devnet / Mainnet
solana program deploy \
  --program-id target/deploy/vaultline_guard-keypair.json \
  target/deploy/vaultline_guard.so

# 4. Verify Local Integration Tests
anchor test

# 5. Launch the Automation Telemetry Service
docker compose up -d --build