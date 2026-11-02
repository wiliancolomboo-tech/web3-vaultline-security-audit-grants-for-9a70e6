# 1. Build and verify Anchor smart contract
anchor build

# 2. Get Program ID
solana address -k target/deploy/vaultline_grant-keypair.json

# 3. Synchronize Program ID into Anchor.toml & lib.rs, then deploy to Devnet
solana config set --url https://api.devnet.solana.com
anchor deploy --program-name vaultline_grant --provider.cluster devnet

# 4. Initialize production environment configuration
cp .env.example .env