#!/usr/bin/env bash
set -euo pipefail

echo "==> [Vaultline] Building and Verifying Anchor Smart Contract..."
anchor build

PROGRAM_ID=$(solana address -k target/deploy/vaultline_audit_grant-keypair.json)
echo "==> Program ID: ${PROGRAM_ID}"

echo "==> Deploying to Solana Mainnet-Beta..."
solana program deploy \
  --program-id target/deploy/vaultline_audit_grant-keypair.json \
  target/deploy/vaultline_audit_grant.so \
  --url https://api.mainnet-beta.solana.com \
  --keypair ~/.config/solana/deployer.json \
  --max-sign-attempts 5

echo "==> Deployment Complete. Initializing Vaultline Pool..."
npx ts-node scripts/init-pool.ts --program-id "${PROGRAM_ID}"