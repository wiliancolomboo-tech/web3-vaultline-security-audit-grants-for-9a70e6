import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { VaultlineAuditGrant } from "../target/types/vaultline_audit_grant";
import { 
  createMint, 
  createAccount, 
  mintTo, 
  getAccount,
  TOKEN_PROGRAM_ID 
} from "@solana/spl-token";
import { PublicKey, Keypair, SystemProgram } from "@solana/web3.js";
import { expect } from "chai";
import * as crypto from "crypto";

describe("Vaultline Security Audit Grants Protocol QA Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.VaultlineAuditGrant as Program<VaultlineAuditGrant>;
  
  const admin = Keypair.generate();
  const colosseumFounder = Keypair.generate();
  const auditor = Keypair.generate();
  const attacker = Keypair.generate();

  let usdcMint: PublicKey;
  let adminUsdcAccount: PublicKey;
  let auditorUsdcAccount: PublicKey;
  let founderUsdcAccount: PublicKey;
  
  const MAX_GRANT_USDC = new anchor.BN(4_000_000_000); // 4,000 USDC (6 decimals)
  const SLA_TIMEOUT_SECS = new anchor.BN(45 * 86400);  // 45 days

  const projectSlug = "colosseum-dex-alpha";
  const dummyProgramId = Keypair.generate().publicKey;
  const commitHash = crypto.createHash("sha256").update("git_commit_7a8b9c").digest();
  const reportHash = crypto.createHash("sha256").update("audit_report_final_v1.0.pdf").digest();

  let configPda: PublicKey;
  let grantPda: PublicKey;
  let escrowVaultPda: PublicKey;

  before(async () => {
    // Airdrop SOL
    for (const kp of [admin, colosseumFounder, auditor, attacker]) {
      const sig = await provider.connection.requestAirdrop(kp.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig);
    }

    // Create Mock USDC Mint
    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6
    );

    // Create Token Accounts
    adminUsdcAccount = await createAccount(provider.connection, admin, usdcMint, admin.publicKey);
    auditorUsdcAccount = await createAccount(provider.connection, auditor, usdcMint, auditor.publicKey);
    founderUsdcAccount = await createAccount(provider.connection, colosseumFounder, usdcMint, colosseumFounder.publicKey);

    // Mint 50,000 USDC to Admin Treasury
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      adminUsdcAccount,
      admin,
      50_000_000_000
    );

    [configPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("protocol_config")],
      program.programId
    );

    [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("audit_grant"), Buffer.from(projectSlug)],
      program.programId
    );

    [escrowVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow_vault"), grantPda.toBuffer()],
      program.programId
    );
  });

  it("1. Initializes the Vaultline Protocol Config", async () => {
    await program.methods
      .initializeProtocol(MAX_GRANT_USDC, SLA_TIMEOUT_SECS)
      .accounts({
        config: configPda,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const config = await program.account.protocolConfig.fetch(configPda);
    expect(config.admin.toBase58()).to.equal(admin.publicKey.toBase58());
    expect(config.maxGrantAmount.toString()).to.equal(MAX_GRANT_USDC.toString());
    expect(config.totalGrantsFunded.toNumber()).to.equal(0);
  });

  it("2. Rejects grant applications exceeding 4,000 USDC ceiling", async () => {
    const invalidSlug = "greedy-project";
    const [invalidGrantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("audit_grant"), Buffer.from(invalidSlug)],
      program.programId
    );
    const excessiveAmount = new anchor.BN(4_000_000_001); // 4,000.000001 USDC

    try {
      await program.methods
        .applyForGrant(
          invalidSlug,
          Array.from(commitHash),
          dummyProgramId,
          excessiveAmount
        )
        .accounts({
          config: configPda,
          grant: invalidGrantPda,
          applicant: colosseumFounder.publicKey,
          systemProgram: SystemProgram.programId,
        })
        .signers([colosseumFounder])
        .rpc();
      expect.fail("Should have thrown GrantLimitExceeded");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("GrantLimitExceeded");
    }
  });

  it("3. Successfully registers valid Colosseum grant application (4,000 USDC)", async () => {
    await program.methods
      .applyForGrant(
        projectSlug,
        Array.from(commitHash),
        dummyProgramId,
        MAX_GRANT_USDC
      )
      .accounts({
        config: configPda,
        grant: grantPda,
        applicant: colosseumFounder.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([colosseumFounder])
      .rpc();

    const grant = await program.account.auditGrant.fetch(grantPda);
    expect(grant.projectSlug).to.equal(projectSlug);
    expect(grant.applicantAuthority.toBase58()).to.equal(colosseumFounder.publicKey.toBase58());
    expect(grant.requestedAmount.toString()).to.equal(MAX_GRANT_USDC.toString());
    expect(grant.status).to.deep.equal({ applied: {} });
  });

  it("4. Prevents unauthorized attacker from approving and funding grant", async () => {
    try {
      await program.methods
        .approveAndFund(MAX_GRANT_USDC)
        .accounts({
          config: configPda,
          grant: grantPda,
          escrowVault: escrowVaultPda,
          adminTokenAccount: adminUsdcAccount,
          usdcMint: usdcMint,
          admin: attacker.publicKey,
          tokenProgram: TOKEN_PROGRAM_ID,
          systemProgram: SystemProgram.programId,
          rent: anchor.web3.SYSVAR_RENT_PUBKEY,
        })
        .signers([attacker])
        .rpc();
      expect.fail("Attacker should not be able to fund without admin key");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("UnauthorizedAdmin");
    }
  });

  it("5. Admin approves and locks 4,000 USDC into PDA escrow vault", async () => {
    await program.methods
      .approveAndFund(MAX_GRANT_USDC)
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        adminTokenAccount: adminUsdcAccount,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
        systemProgram: SystemProgram.programId,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([admin])
      .rpc();

    const vaultAccount = await getAccount(provider.connection, escrowVaultPda);
    expect(vaultAccount.amount.toString()).to.equal(MAX_GRANT_USDC.toString());

    const grant = await program.account.auditGrant.fetch(grantPda);
    expect(grant.status).to.deep.equal({ approvedAndFunded: {} });
  });

  it("6. Assigns vetted security auditor pubkey", async () => {
    await program.methods
      .assignAuditor(auditor.publicKey)
      .accounts({
        config: configPda,
        grant: grantPda,
        admin: admin.publicKey,
      })
      .signers([admin])
      .rpc();

    const grant = await program.account.auditGrant.fetch(grantPda);
    expect(grant.auditorPubkey.toBase58()).to.equal(auditor.publicKey.toBase58());
    expect(grant.status).to.deep.equal({ auditorAssigned: {} });
  });

  it("7. Auditor notarizes SHA-256 audit report and vulnerability taxonomy", async () => {
    await program.methods
      .submitAuditReport(
        Array.from(reportHash),
        1, // 1 Critical
        2, // 2 Highs
        4, // 4 Mediums
        7  // 7 Lows
      )
      .accounts({
        grant: grantPda,
        auditorPubkey: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    const grant = await program.account.auditGrant.fetch(grantPda);
    expect(grant.status).to.deep.equal({ reportSubmitted: {} });
    expect(grant.criticalCount).to.equal(1);
    expect(grant.highCount).to.equal(2);
  });

  it("8. Releases Milestone 1 (50% tranche: 80% to Auditor, 20% to Project)", async () => {
    // Milestone 1 tranche = 2,000 USDC. Auditor BPS = 8000 (80% = 1,600 USDC), Project = 400 USDC
    await program.methods
      .disburseMilestone(1, 8000)
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        auditorTokenAccount: auditorUsdcAccount,
        projectTokenAccount: founderUsdcAccount,
        admin: admin.publicKey,
        applicantAuthority: colosseumFounder.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorUsdcAccount);
    const founderBalance = await getAccount(provider.connection, founderUsdcAccount);
    const vaultBalance = await getAccount(provider.connection, escrowVaultPda);

    expect(auditorBalance.amount.toString()).to.equal("1600000000"); // 1,600 USDC
    expect(founderBalance.amount.toString()).to.equal("400000000");  // 400 USDC
    expect(vaultBalance.amount.toString()).to.equal("2000000000");   // 2,000 USDC remaining
  });

  it("9. Releases Milestone 2 (Final Remediation Signoff & Full Settlement)", async () => {
    // Milestone 2 tranche = remaining 2,000 USDC. Split 50/50 (5000 bps)
    await program.methods
      .disburseMilestone(2, 5000)
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        auditorTokenAccount: auditorUsdcAccount,
        projectTokenAccount: founderUsdcAccount,
        admin: admin.publicKey,
        applicantAuthority: colosseumFounder.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const vaultBalance = await getAccount(provider.connection, escrowVaultPda);
    expect(vaultBalance.amount.toString()).to.equal("0");

    const grant = await program.account.auditGrant.fetch(grantPda);
    expect(grant.status).to.deep.equal({ completed: {} });
    expect(grant.disbursedAmount.toString()).to.equal(MAX_GRANT_USDC.toString());
  });
});