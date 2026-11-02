# Target Solana Devnet
solana config set --url https://api.devnet.solana.com

# Deploy the program bytecode
anchor deploy --provider.cluster devnet

# Verify program account
solana program show <PROGRAM_ID>