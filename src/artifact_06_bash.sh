# 1. Build and verify Anchor binary
anchor build

# 2. Deploy to Solana Devnet / Mainnet-Beta
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --program-name vaultline_grants --program-keypair target/deploy/vaultline_grants-keypair.json

# 3. Initialize Production USDC Escrow Configuration
yarn ts-node src/cli/init-escrow.ts \
  --project "colosseum-winner-v1" \
  --repo "https://github.com/colosseum/protocol" \
  --grant 4000000000