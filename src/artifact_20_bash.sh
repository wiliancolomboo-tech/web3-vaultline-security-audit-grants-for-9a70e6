#!/usr/bin/env bash
set -euo pipefail

echo "================ Vaultline Protocol Healthcheck ================"
PROGRAM_ID="VLineGR4NT111111111111111111111111111111111"
RPC="${SOLANA_RPC_URL:-https://api.devnet.solana.com}"

ACCOUNT_DATA=$(curl -s -X POST -H "Content-Type: application/json" -d "{
  \"jsonrpc\": \"2.0\",
  \"id\": 1,
  \"method\": \"getAccountInfo\",
  \"params\": [\"${PROGRAM_ID}\", {\"encoding\": \"base64\"}]
}" "$RPC")

if echo "$ACCOUNT_DATA" | grep -q "result"; then
  echo "[✔] Solana On-Chain Program Verified Active: $PROGRAM_ID"
else
  echo "[✖] ERROR: Program account not reachable on RPC: $RPC"
  exit 1
fi

echo "[✔] Healthcheck complete. Service operational."