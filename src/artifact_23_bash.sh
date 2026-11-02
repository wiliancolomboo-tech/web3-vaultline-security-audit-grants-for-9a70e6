# ==============================================================================
# 1. BUILD & TEST ANCHOR PROGRAM
# ==============================================================================
anchor build
anchor test

# ==============================================================================
# 2. DEPLOY ON SOLANA DEVNET / MAINNET
# ==============================================================================
solana program deploy \
  --program-id target/deploy/vaultline_grants-keypair.json \
  target/deploy/vaultline_grants.so \
  --url devnet

# ==============================================================================
# 3. INITIALIZE PROGRAM STATE (USDC Mint on Devnet: 4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU)
# ==============================================================================
vaultline init \
  --keypair ~/.config/solana/id.json \
  --mint 4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU

# ==============================================================================
# 4. DISBURSE & VERIFY FIRST COLOSSEUM AUDIT GRANT
# ==============================================================================
vaultline apply \
  --keypair ~/.config/solana/id.json \
  --grantee <COLOSSEUM_FOUNDER_PUBKEY> \
  --auditor <CERTIFIED_AUDITOR_PUBKEY> \
  --mint 4zMMC9srt5Ri5X14GAgXhaHii3GnPAEERYPJgZJDncDU \
  --amount 4000 \
  --days 30