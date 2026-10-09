# 1. Clone repository & install dependencies
git clone https://github.com/vaultline-protocol/colosseum-audit-grants.git
cd colosseum-audit-grants
npm install

# 2. Build on-chain Solana program
anchor build

# 3. Deploy to Solana Devnet / Mainnet-Beta
solana config set --url https://api.devnet.solana.com
solana airdrop 2
anchor deploy --provider.cluster devnet

# 4. Initialize Program Pool with USDC Mint
npx ts-node src/cli.ts init-pool \
  --admin-keypair ./keys/admin.json \
  --usdc-mint 4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU