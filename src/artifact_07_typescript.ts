import { Command } from 'commander';
import * as anchor from '@coral-xyz/anchor';
import { PublicKey, Keypair, Connection } from '@solana/web3.js';
import { getOrCreateAssociatedTokenAccount } from '@solana/spl-token';
import * as crypto from 'crypto';
import * as fs from 'fs';
import * as dotenv from 'dotenv';
import { EnterpriseNotificationService } from './notifier';

dotenv.config();

const programId = new PublicKey(process.env.PROGRAM_ID || 'VLinEK3yZ5Xg8V9Pj1R7u1z5iY7L8sZ8xQGv9aK8oJ2');
const notifier = new EnterpriseNotificationService();

function loadKeypair(path: string): Keypair {
  const secretKey = JSON.parse(fs.readFileSync(path, 'utf8'));
  return Keypair.fromSecretKey(Uint8Array.from(secretKey));
}

const program = new Command();
program
  .name('vaultline-cli')
  .description('Institutional CLI for Vaultline Colosseum Audit Grants')
  .version('1.0.0');

program
  .command('init-grant')
  .requiredOption('-k, --keypair <path>', 'Path to Admin keypair')
  .requiredOption('-p, --project-id <string>', 'Colosseum Project Identifier')
  .requiredOption('-a, --auditor <pubkey>', 'Auditor Public Key')
  .requiredOption('-l, --project-lead <pubkey>', 'Project Lead Public Key')
  .requiredOption('-m, --mint <pubkey>', 'USDC Mint Address')
  .action(async (opts) => {
    const adminKeypair = loadKeypair(opts.keypair);
    const connection = new Connection(process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com', 'confirmed');
    const wallet = new anchor.Wallet(adminKeypair);
    const provider = new anchor.AnchorProvider(connection, wallet, { commitment: 'confirmed' });
    anchor.setProvider(provider);

    const idl = await anchor.Program.fetchIdl(programId, provider);
    if (!idl) throw new Error('Failed to fetch IDL');
    const client = new anchor.Program(idl, provider);

    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('vaultline_grant'), adminKeypair.publicKey.toBuffer(), Buffer.from(opts.projectId)],
      programId
    );

    const [vaultTokenAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from('grant_vault'), grantPda.toBuffer()],
      programId
    );

    const commitHash = crypto.createHash('sha256').update(opts.projectId + Date.now().toString()).digest();

    console.log(`[Vaultline] Initializing Grant for: ${opts.projectId}`);
    console.log(`Grant PDA: ${grantPda.toBase58()}`);

    const tx = await client.methods
      .initializeGrant(opts.projectId, Array.from(commitHash))
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        usdcMint: new PublicKey(opts.mint),
        auditor: new PublicKey(opts.auditor),
        projectLead: new PublicKey(opts.projectLead),
        admin: adminKeypair.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .rpc();

    console.log(`SUCCESS: Initialized with Tx Signature: ${tx}`);

    await notifier.broadcast({
      eventType: 'INITIALIZED',
      projectId: opts.projectId,
      grantPubkey: grantPda.toBase58(),
      txSignature: tx,
    });
  });

program
  .command('fund-grant')
  .requiredOption('-k, --keypair <path>', 'Path to Funder keypair')
  .requiredOption('-p, --project-id <string>', 'Colosseum Project Identifier')
  .requiredOption('-a, --admin-pubkey <pubkey>', 'Admin Public Key')
  .requiredOption('-m, --mint <pubkey>', 'USDC Mint Address')
  .requiredOption('-u, --amount-usdc <number>', 'Amount of USDC (e.g. 4000)')
  .action(async (opts) => {
    const funderKeypair = loadKeypair(opts.keypair);
    const connection = new Connection(process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com', 'confirmed');
    const wallet = new anchor.Wallet(funderKeypair);
    const provider = new anchor.AnchorProvider(connection, wallet, { commitment: 'confirmed' });
    anchor.setProvider(provider);

    const idl = await anchor.Program.fetchIdl(programId, provider);
    if (!idl) throw new Error('Failed to fetch IDL');
    const client = new anchor.Program(idl, provider);

    const adminPubkey = new PublicKey(opts.adminPubkey);
    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('vaultline_grant'), adminPubkey.toBuffer(), Buffer.from(opts.projectId)],
      programId
    );

    const [vaultTokenAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from('grant_vault'), grantPda.toBuffer()],
      programId
    );

    const usdcMint = new PublicKey(opts.mint);
    const funderTokenAcc = await getOrCreateAssociatedTokenAccount(
      connection,
      funderKeypair,
      usdcMint,
      funderKeypair.publicKey
    );

    const rawAmount = new anchor.BN(opts.amountUsdc).mul(new anchor.BN(1_000_000));

    console.log(`[Vaultline] Funding Grant PDA ${grantPda.toBase58()} with ${opts.amountUsdc} USDC`);

    const tx = await client.methods
      .fundEscrow(rawAmount)
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        funderTokenAccount: funderTokenAcc.address,
        funder: funderKeypair.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .rpc();

    console.log(`SUCCESS: Escrow Funded with Tx: ${tx}`);

    await notifier.broadcast({
      eventType: 'FUNDED',
      projectId: opts.projectId,
      grantPubkey: grantPda.toBase58(),
      amountUSDC: parseFloat(opts.amountUsdc),
      txSignature: tx,
    });
  });

program
  .command('submit-artifact')
  .requiredOption('-k, --keypair <path>', 'Auditor Keypair')
  .requiredOption('-p, --project-id <string>', 'Colosseum Project ID')
  .requiredOption('-a, --admin-pubkey <pubkey>', 'Admin Public Key')
  .requiredOption('-m, --milestone <number>', 'Milestone index: 1 (Scope), 2 (Findings), 3 (Final)')
  .requiredOption('-f, --file-path <path>', 'Path to report markdown/pdf to hash')
  .action(async (opts) => {
    const auditorKeypair = loadKeypair(opts.keypair);
    const connection = new Connection(process.env.SOLANA_RPC_URL || 'https://api.devnet.solana.com', 'confirmed');
    const wallet = new anchor.Wallet(auditorKeypair);
    const provider = new anchor.AnchorProvider(connection, wallet, { commitment: 'confirmed' });
    anchor.setProvider(provider);

    const idl = await anchor.Program.fetchIdl(programId, provider);
    if (!idl) throw new Error('Failed to fetch IDL');
    const client = new anchor.Program(idl, provider);

    const adminPubkey = new PublicKey(opts.adminPubkey);
    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from('vaultline_grant'), adminPubkey.toBuffer(), Buffer.from(opts.projectId)],
      programId
    );

    const fileBuffer = fs.readFileSync(opts.filePath);
    const hash = crypto.createHash('sha256').update(fileBuffer).digest();

    let milestoneVariant = { scopeApproved: {} };
    if (opts.milestone === '2') milestoneVariant = { findingsTriaged: {} };
    if (opts.milestone === '3') milestoneVariant = { remediationsSignedOff: {} };

    console.log(`[Vaultline] Submitting Artifact Hash: ${hash.toString('hex')}`);

    const tx = await client.methods
      .submitMilestoneArtifact(milestoneVariant, Array.from(hash))
      .accounts({
        grantState: grantPda,
        auditor: auditorKeypair.publicKey,
      })
      .rpc();

    console.log(`SUCCESS: Artifact Recorded with Tx: ${tx}`);

    await notifier.broadcast({
      eventType: 'ARTIFACT_SUBMITTED',
      projectId: opts.projectId,
      grantPubkey: grantPda.toBase58(),
      milestoneName: `Milestone ${opts.milestone}`,
      artifactHash: hash.toString('hex'),
      txSignature: tx,
    });
  });

program.parse(process.argv);