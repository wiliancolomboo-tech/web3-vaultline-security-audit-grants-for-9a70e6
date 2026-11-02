# Configure to devnet or mainnet-beta
solana config set --url https://api.devnet.solana.com

# Build the Anchor program binary
anchor build

# Deploy to Solana
solana program deploy ./target/deploy/vaultline_grant_core.so \
  --program-id ./target/deploy/vaultline_grant_core-keypair.json