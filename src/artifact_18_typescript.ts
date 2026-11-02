import { Command } from "commander";
import * as anchor from "@coral-xyz/anchor";
import { PublicKey, Keypair, Connection, clusterApiUrl } from "@solana/web3.js";
import { getOrCreateAssociatedTokenAccount } from "@solana/spl-token";
import * as fs from "fs";
import * as dotenv from "dotenv";
import { v4 as uuidv4 } from "uuid";
import { ArtifactHasher } from "./services/hasher";
import { EnterpriseNotifier } from "./services/notifier";

dotenv.config();

const programId = new PublicKey(
  process.env.PROGRAM_ID || "V1tLN8Z3jQdKzVw7dM5Lq2YkP8g1mE4sN6uRxT9aBC1"
);

function loadKeypair(path: string): Keypair {
  const secretKey = JSON.parse(fs.readFileSync(path, "utf-8"));
  return Keypair.fromSecretKey(new Uint8Array(secretKey));
}

const program = new Command();
const notifier = new EnterpriseNotifier();

program
  .name("vaultline")
  .description("Vaultline Security Audit Grants Automation Agent")
  .version("1.0.0");

program
  .command("apply")
  .description("Apply and register a new Colosseum project grant account")
  .requiredOption("-k, --keypair <path>", "Payer keypair JSON path")
  .requiredOption("-g, --grantee <pubkey>", "Grantee founder public key")
  .requiredOption("-a, --auditor <pubkey>", "Certified auditor public key")
  .requiredOption("-m, --mint <pubkey>", "USDC Mint address")
  .option("-u, --amount <number>", "USDC allocation (max 4000)", "4000")
  .option("-d, --days <number>", "SLA duration in days", "30")
  .action(async (opts) => {
    const payer = loadKeypair(opts.keypair);
    const connection = new Connection(process.env.SOLANA_RPC_URL || clusterApiUrl("devnet"), "confirmed");
    const wallet = new anchor.Wallet(payer);
    const provider = new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" });
    anchor.setProvider(provider);

    const uuid = uuidv4();
    const grantIdBytes = ArtifactHasher.parseUuidTo16Bytes(uuid);

    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), Buffer.from(grantIdBytes)],
      programId
    );
    const [escrowPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), grantPda.toBuffer()],
      programId
    );

    const amountLamports = new anchor.BN(parseFloat(opts.amount) * 1_000_000);
    const slaSeconds = new anchor.BN(parseInt(opts.days, 10) * 86400);

    console.log(`Submitting application for Grant UUID: ${uuid}`);
    console.log(`Grant PDA: ${grantPda.toBase58()}`);
    console.log(`Escrow PDA: ${escrowPda.toBase58()}`);

    // Execute Anchor instruction
    const idl = await anchor.Program.fetchIdl(programId, provider);
    if (!idl) throw new Error("IDL not found for program ID");
    const anchorProgram = new anchor.Program(idl, provider);

    const tx = await anchorProgram.methods
      .applyGrant(grantIdBytes, amountLamports, slaSeconds)
      .accounts({
        config: PublicKey.findProgramAddressSync([Buffer.from("config")], programId)[0],
        grant: grantPda,
        escrowVault: escrowPda,
        usdcMint: new PublicKey(opts.mint),
        granteeAuthority: new PublicKey(opts.grantee),
        auditor: new PublicKey(opts.auditor),
        payer: payer.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([payer])
      .rpc();

    console.log(`Grant registered successfully. Tx signature: ${tx}`);
    await notifier.broadcast({
      grantId: uuid,
      projectName: "Colosseum Grantee Project",
      auditorAddress: opts.auditor,
      amountUsdc: parseFloat(opts.amount),
      stage: "FUNDED",
      explorerUrl: `https://explorer.solana.com/tx/${tx}?cluster=devnet`,
    });
  });

program
  .command("submit-report")
  .description("Auditor commits report SHA-256 hash to initiate Milestone 1 release")
  .requiredOption("-k, --keypair <path>", "Auditor keypair JSON path")
  .requiredOption("-i, --grant-uuid <uuid>", "Grant UUID")
  .requiredOption("-f, --file <path>", "Path to Audit Report PDF")
  .action(async (opts) => {
    const auditor = loadKeypair(opts.keypair);
    const connection = new Connection(process.env.SOLANA_RPC_URL || clusterApiUrl("devnet"), "confirmed");
    const wallet = new anchor.Wallet(auditor);
    const provider = new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" });
    anchor.setProvider(provider);

    const reportHash = ArtifactHasher.hashFile(opts.file);
    const grantIdBytes = ArtifactHasher.parseUuidTo16Bytes(opts.grantUuid);

    const [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), Buffer.from(grantIdBytes)],
      programId
    );

    const idl = await anchor.Program.fetchIdl(programId, provider);
    if (!idl) throw new Error("IDL not found");
    const anchorProgram = new anchor.Program(idl, provider);

    const tx = await anchorProgram.methods
      .submitReport(Array.from(reportHash))
      .accounts({
        grant: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    console.log(`Report digest committed. Tx: ${tx}`);
    await notifier.broadcast({
      grantId: opts.grantUuid,
      projectName: "Colosseum Finalist",
      auditorAddress: auditor.publicKey.toBase58(),
      amountUsdc: 2800,
      stage: "AUDIT_SUBMITTED",
      reportHash: reportHash.toString("hex"),
      explorerUrl: `https://explorer.solana.com/tx/${tx}?cluster=devnet`,
    });
  });

program.parse(process.argv);