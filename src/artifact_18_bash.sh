# Set network cluster
solana config set --url https://api.devnet.solana.com

# Build Anchor project with verifiable BPF build
anchor build

# Sync Program ID into lib.rs and Anchor.toml
anchor keys sync

# Deploy Program
anchor deploy --program-name vaultline_audit_grant