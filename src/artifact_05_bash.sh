# 1. Clone repository & install dependencies
git clone https://github.com/vaultline-security/vaultline-audit-escrow.git
cd vaultline-audit-escrow
npm install

# 2. Build Anchor Smart Contract & generate TypeIDL
anchor build

# 3. Deploy to Solana Devnet
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 4. Run full test suite with determinism checks
anchor test