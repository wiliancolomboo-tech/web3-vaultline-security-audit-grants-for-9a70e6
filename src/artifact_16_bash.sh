# 1. Build Solana Anchor Program
anchor build

# 2. Synchronize Program ID keys
solana address -k target/deploy/vaultline_audit-keypair.json
# Copy printed address into Anchor.toml and declare_id!() in lib.rs

# 3. Deploy to Solana Devnet
solana program deploy \
  --program-id target/deploy/vaultline_audit-keypair.json \
  target/deploy/vaultline_audit.so \
  --url-url https://api.devnet.solana.com

# 4. Initialize Program Configuration (4,000 USDC standard)
npx ts-node cli/admin-init.ts --grant-amount 4000000000