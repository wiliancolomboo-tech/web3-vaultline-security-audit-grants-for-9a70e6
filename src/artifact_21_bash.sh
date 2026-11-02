# 1. Setup Solana CLI Configuration
solana config set --url https://api.devnet.solana.com
solana airdrop 2

# 2. Build program binary with reproducible Anchor container
anchor build

# 3. Synchronize generated program ID
anchor keys sync

# 4. Deploy program to network
solana program deploy target/deploy/vaultline_sge.so

# 5. Initialize the Global Protocol Configuration
yarn run cli initialize-config \
  --auditor <VAULTLINE_AUDITOR_PUBKEY> \
  --mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v \
  --keypair ~/.config/solana/id.json