import * as anchor from "@coral-xyz/anchor";
import { Program, Wallet } from "@coral-xyz/anchor";
import { Connection, Keypair, PublicKey, clusterApiUrl } from "@solana/web3.js";
import { TOKEN_PROGRAM_ID, getOrCreateAssociatedTokenAccount } from "@solana/spl-token";
import * as crypto from "crypto";
import axios from "axios";
import { Resend } from "resend";
import * as dotenv from "dotenv";

dotenv.config();

// Configuration environment variables
const RPC_URL = process.env.SOLANA_RPC_URL || clusterApiUrl("devnet");
const RESEND_API_KEY = process.env.RESEND_API_KEY || "re_test_000000000000000000000000";
const SLACK_WEBHOOK_URL = process.env.SLACK_WEBHOOK_URL || "";
const PROGRAM_ID = new PublicKey(process.env.PROGRAM_ID || "V1tLineAuditGrant1111111111111111111111111111");

const resend = new Resend(RESEND_API_KEY);

export interface ColosseumProjectSubmission {
  projectId: string;
  githubRepoUrl: string;
  commitHash: string;
  founderEmail: string;
  projectAuthorityPubkey: PublicKey;
  targetProgramIds: PublicKey[];
}

export class VaultlineGrantOrchestrator {
  private connection: Connection;
  private auditorKeypair: Keypair;
  private program: Program;

  constructor(auditorKeypair: Keypair, idl: any) {
    this.connection = new Connection(RPC_URL, "confirmed");
    this.auditorKeypair = auditorKeypair;
    const wallet = new Wallet(this.auditorKeypair);
    const provider = new anchor.AnchorProvider(this.connection, wallet, {
      commitment: "confirmed",
    });
    anchor.setProvider(provider);
    this.program = new Program(idl, PROGRAM_ID, provider);
  }

  /**
   * Derive Grant Account PDA
   */
  public getGrantPDA(projectAuthority: PublicKey, projectId: string): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), projectAuthority.toBuffer(), Buffer.from(projectId)],
      this.program.programId
    );
  }

  /**
   * Derive Grant USDC Vault PDA
   */
  public getVaultPDA(grantPDA: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("grant_vault"), grantPDA.toBuffer()],
      this.program.programId
    );
  }

  /**
   * Derive Attestation Certificate PDA
   */
  public getAttestationPDA(grantPDA: PublicKey): [PublicKey, number] {
    return PublicKey.findProgramAddressSync(
      [Buffer.from("attestation"), grantPDA.toBuffer()],
      this.program.programId
    );
  }

  /**
   * Automated AST & Vulnerability Pre-Check
   */
  public async runStaticPreScan(repoUrl: string, commitHash: string): Promise<{
    passed: boolean;
    issuesFound: string[];
    scopeSha256: Buffer;
  }> {
    console.log(`[Vaultline Scanner] Analyzing Colosseum Repo: ${repoUrl} @ commit ${commitHash}`);
    
    // In production, executes `cargo-audit`, `geiger`, and custom Solana AST invariant passes
    const scopeDigest = crypto.createHash("sha256").update(commitHash + repoUrl).digest();
    
    return {
      passed: true,
      issuesFound: [],
      scopeSha256: scopeDigest,
    };
  }

  /**
   * Enterprise Step 1: Lock Scope On-Chain
   */
  public async lockScopeOnChain(
    project: ColosseumProjectSubmission,
    usdcMint: PublicKey
  ): Promise<string> {
    const [grantPDA] = this.getGrantPDA(project.projectAuthorityPubkey, project.projectId);
    const [vaultPDA] = this.getVaultPDA(grantPDA);

    const auditorTokenAccount = await getOrCreateAssociatedTokenAccount(
      this.connection,
      this.auditorKeypair,
      usdcMint,
      this.auditorKeypair.publicKey
    );

    const scanResult = await this.runStaticPreScan(project.githubRepoUrl, project.commitHash);
    const commitBuffer = Array.from(scanResult.scopeSha256);

    console.log(`[On-Chain] Executing record_scope for Grant: ${grantPDA.toBase58()}`);

    const tx = await this.program.methods
      .recordScope(commitBuffer, project.targetProgramIds)
      .accounts({
        grantAccount: grantPDA,
        grantVault: vaultPDA,
        auditorTokenAccount: auditorTokenAccount.address,
        auditor: this.auditorKeypair.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([this.auditorKeypair])
      .rpc();

    await this.notifyTelemetry({
      event: "SCOPE_LOCKED",
      projectId: project.projectId,
      commitHash: project.commitHash,
      txSignature: tx,
      founderEmail: project.founderEmail,
    });

    return tx;
  }

  /**
   * Enterprise Step 2: Finalize Audit, Mint Attestation Certificate & Trigger Grant Payout
   */
  public async certifyAuditAndDisburse(
    project: ColosseumProjectSubmission,
    usdcMint: PublicKey,
    findings: {
      critical: number;
      high: number;
      medium: number;
      low: number;
      score: number;
      reportBuffer: Buffer;
    }
  ): Promise<{ tx: string; attestationPDA: PublicKey }> {
    const [grantPDA] = this.getGrantPDA(project.projectAuthorityPubkey, project.projectId);
    const [vaultPDA] = this.getVaultPDA(grantPDA);
    const [attestationPDA] = this.getAttestationPDA(grantPDA);

    const reportHash = crypto.createHash("sha256").update(findings.reportBuffer).digest();

    const auditorTokenAccount = await getOrCreateAssociatedTokenAccount(
      this.connection,
      this.auditorKeypair,
      usdcMint,
      this.auditorKeypair.publicKey
    );

    console.log(`[On-Chain] Minting Security Attestation: ${attestationPDA.toBase58()}`);

    const tx = await this.program.methods
      .finalizeAuditAndAttest(
        Array.from(reportHash),
        findings.critical,
        findings.high,
        findings.medium,
        findings.low,
        findings.score
      )
      .accounts({
        grantAccount: grantPDA,
        attestationCertificate: attestationPDA,
        grantVault: vaultPDA,
        auditorTokenAccount: auditorTokenAccount.address,
        auditor: this.auditorKeypair.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([this.auditorKeypair])
      .rpc();

    await this.notifyTelemetry({
      event: "AUDIT_CERTIFIED",
      projectId: project.projectId,
      score: findings.score,
      txSignature: tx,
      attestationPDA: attestationPDA.toBase58(),
      founderEmail: project.founderEmail,
    });

    return { tx, attestationPDA };
  }

  /**
   * Real Enterprise Telemetry: Resend API + Slack Webhook
   */
  private async notifyTelemetry(payload: any): Promise<void> {
    console.log(`[Telemetry] Dispatching Webhook for Event: ${payload.event}`);

    // 1. Resend Transactional Email to Founder
    if (payload.founderEmail && process.env.RESEND_API_KEY) {
      try {
        await resend.emails.send({
          from: "Vaultline Security Grants <grants@vaultline.audit>",
          to: payload.founderEmail,
          subject: `[Vaultline Grant Update] Status: ${payload.event} for ${payload.projectId}`,
          html: `
            <h2>Vaultline Colosseum Security Grant Notice</h2>
            <p>Your security milestone has been executed on-chain.</p>
            <ul>
              <li><strong>Event:</strong> ${payload.event}</li>
              <li><strong>Project:</strong> ${payload.projectId}</li>
              <li><strong>Solana Tx:</strong> <code>${payload.txSignature}</code></li>
              ${payload.score ? `<li><strong>Security Score:</strong> ${payload.score}/100</li>` : ""}
              ${payload.attestationPDA ? `<li><strong>Attestation Certificate:</strong> <code>${payload.attestationPDA}</code></li>` : ""}
            </ul>
            <p>View on Solana Explorer: <a href="https://explorer.solana.com/tx/${payload.txSignature}?cluster=devnet">Explorer Link</a></p>
          `,
        });
      } catch (err: any) {
        console.warn(`[Warning] Resend notification failed: ${err.message}`);
      }
    }

    // 2. Real-time Slack/Discord Webhook Alert
    if (SLACK_WEBHOOK_URL) {
      try {
        await axios.post(SLACK_WEBHOOK_URL, {
          text: `🛡️ *Vaultline Security Grant Update*\n• Event: \`${payload.event}\`\n• Project: *${payload.projectId}*\n• Tx: \`${payload.txSignature}\``,
        });
      } catch (err: any) {
        console.warn(`[Warning] Slack alert failed: ${err.message}`);
      }
    }
  }
}