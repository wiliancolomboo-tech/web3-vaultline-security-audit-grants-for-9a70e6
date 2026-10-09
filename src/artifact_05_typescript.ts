import * as anchor from "@coral-xyz/anchor";
import { Program, BN } from "@coral-xyz/anchor";
import { VaultlineGrants } from "../target/types/vaultline_grants";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { assert, expect } from "chai";
import * as crypto from "crypto";

describe("vaultline_grants test suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.VaultlineGrants as Program<VaultlineGrants>;

  const admin = anchor.web3.Keypair.generate();
  const funder = anchor.web3.Keypair.generate();
  const applicant = anchor.web3.Keypair.generate();
  const auditor = anchor.web3.Keypair.generate();
  const maliciousActor = anchor.web3.Keypair.generate();

  let usdcMint: anchor.web3.PublicKey;
  let funderTokenAccount: anchor.web3.PublicKey;
  let auditorTokenAccount: anchor.web3.PublicKey;
  let adminTokenAccount: anchor.web3.PublicKey;
  let maliciousTokenAccount: anchor.web3.PublicKey;

  let poolPda: anchor.web3.PublicKey;
  let poolBump: number;

  const projectId = crypto.randomBytes(32);
  const projectName = "Colosseum-DeFi-Perps";
  const repoHash = crypto.randomBytes(32);

  let grantPda: anchor.web3.PublicKey;
  let vaultPda: anchor.web3.PublicKey;

  before(async () => {
    // Fund all keypairs with native SOL
    const airdropSigners = [admin, funder, applicant, auditor, maliciousActor];
    for (const signer of airdropSigners) {
      const sig = await provider.connection.requestAirdrop(signer.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL);
      await provider.connection.confirmTransaction(sig);
    }

    // Create Mock 6-Decimal USDC Mint
    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6
    );

    // Initialize Token Accounts
    funderTokenAccount = await createAccount(provider.connection, funder, usdcMint, funder.publicKey);
    auditorTokenAccount = await createAccount(provider.connection, auditor, usdcMint, auditor.publicKey);
    adminTokenAccount = await createAccount(provider.connection, admin, usdcMint, admin.publicKey);
    maliciousTokenAccount = await createAccount(provider.connection, maliciousActor, usdcMint, maliciousActor.publicKey);

    // Mint 10,000 USDC to Funder
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      funderTokenAccount,
      admin,
      10_000_000_000
    );

    // Derive Global Pool PDA
    [poolPda, poolBump] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("grant_pool")],
      program.programId
    );

    // Derive Grant and Vault PDAs
    [grantPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("audit_grant"), projectId],
      program.programId
    );

    [vaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("grant_vault"), projectId],
      program.programId
    );
  });

  it("1. Initializes global GrantPool treasury", async () => {
    await program.methods
      .initializePool(poolBump)
      .accounts({
        grantPool: poolPda,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const pool = await program.account.grantPool.fetch(poolPda);
    assert.isTrue(pool.admin.equals(admin.publicKey));
    assert.isTrue(pool.usdcMint.equals(usdcMint));
    assert.equal(pool.totalAllocated.toNumber(), 0);
    assert.equal(pool.grantCount.toNumber(), 0);
  });

  it("2. Creates audit grant and deposits 4,000 USDC into PDA vault", async () => {
    await program.methods
      .createAuditGrant(
        Array.from(projectId),
        projectName,
        Array.from(repoHash)
      )
      .accounts({
        grantPool: poolPda,
        auditGrant: grantPda,
        grantVault: vaultPda,
        usdcMint: usdcMint,
        funderTokenAccount: funderTokenAccount,
        admin: admin.publicKey,
        funder: funder.publicKey,
        applicant: applicant.publicKey,
        auditor: auditor.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([admin, funder])
      .rpc();

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    assert.equal(grantAccount.totalAllocated.toNumber(), 4_000_000_000);
    assert.equal(grantAccount.totalDisbursed.toNumber(), 0);
    assert.isTrue(grantAccount.auditor.equals(auditor.publicKey));
    assert.isTrue(grantAccount.applicant.equals(applicant.publicKey));

    const vaultBalance = await getAccount(provider.connection, vaultPda);
    assert.equal(Number(vaultBalance.amount), 4_000_000_000);
  });

  it("3. Rejects deliverable submission by unauthorized malicious user", async () => {
    const dummyHash = crypto.randomBytes(32);
    try {
      await program.methods
        .submitMilestoneDeliverable(0, Array.from(dummyHash))
        .accounts({
          auditGrant: grantPda,
          auditor: maliciousActor.publicKey,
        })
        .signers([maliciousActor])
        .rpc();
      assert.fail("Should have failed on unauthorized auditor");
    } catch (err: any) {
      expect(err.message).to.include("ConstraintRaw") || expect(err.message).to.include("has_one");
    }
  });

  it("4. Rejects out-of-order milestone progression (Milestone 2 before Milestone 0)", async () => {
    const dummyHash = crypto.randomBytes(32);
    try {
      await program.methods
        .submitMilestoneDeliverable(1, Array.from(dummyHash))
        .accounts({
          auditGrant: grantPda,
          auditor: auditor.publicKey,
        })
        .signers([auditor])
        .rpc();
      assert.fail("Should have blocked sequence violation");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("MilestoneSequenceViolation");
    }
  });

  it("5. Completes Milestone 1: Scope & Threat Model (Disburses 1,000 USDC)", async () => {
    const scopeDeliverableHash = crypto.randomBytes(32);

    // Auditor submits deliverable
    await program.methods
      .submitMilestoneDeliverable(0, Array.from(scopeDeliverableHash))
      .accounts({
        auditGrant: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    // Admin approves & triggers 1,000 USDC payout
    await program.methods
      .approveAndDisburse(0)
      .accounts({
        grantPool: poolPda,
        auditGrant: grantPda,
        grantVault: vaultPda,
        auditorTokenAccount: auditorTokenAccount,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    assert.equal(Number(auditorBalance.amount), 1_000_000_000);

    const vaultBalance = await getAccount(provider.connection, vaultPda);
    assert.equal(Number(vaultBalance.amount), 3_000_000_000);
  });

  it("6. Completes Milestone 2: Draft Report (Disburses 2,000 USDC)", async () => {
    const draftHash = crypto.randomBytes(32);

    await program.methods
      .submitMilestoneDeliverable(1, Array.from(draftHash))
      .accounts({
        auditGrant: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    await program.methods
      .approveAndDisburse(1)
      .accounts({
        grantPool: poolPda,
        auditGrant: grantPda,
        grantVault: vaultPda,
        auditorTokenAccount: auditorTokenAccount,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    assert.equal(Number(auditorBalance.amount), 3_000_000_000);

    const vaultBalance = await getAccount(provider.connection, vaultPda);
    assert.equal(Number(vaultBalance.amount), 1_000_000_000);
  });

  it("7. Completes Milestone 3: Final Certificate (Disburses remaining 1,000 USDC)", async () => {
    const finalReportHash = crypto.randomBytes(32);

    await program.methods
      .submitMilestoneDeliverable(2, Array.from(finalReportHash))
      .accounts({
        auditGrant: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    await program.methods
      .approveAndDisburse(2)
      .accounts({
        grantPool: poolPda,
        auditGrant: grantPda,
        grantVault: vaultPda,
        auditorTokenAccount: auditorTokenAccount,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    assert.equal(Number(auditorBalance.amount), 4_000_000_000);

    const vaultBalance = await getAccount(provider.connection, vaultPda);
    assert.equal(Number(vaultBalance.amount), 0);

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    assert.isTrue("completed" in grantAccount.status);
  });

  it("8. Validates Cancellation: Reclaims unused grant USDC on abandonment", async () => {
    // Initialize a second grant that will be cancelled halfway
    const proj2Id = crypto.randomBytes(32);
    const [proj2GrantPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("audit_grant"), proj2Id],
      program.programId
    );
    const [proj2VaultPda] = anchor.web3.PublicKey.findProgramAddressSync(
      [Buffer.from("grant_vault"), proj2Id],
      program.programId
    );

    // Fund project 2
    await program.methods
      .createAuditGrant(
        Array.from(proj2Id),
        "Abandoned-Project",
        Array.from(crypto.randomBytes(32))
      )
      .accounts({
        grantPool: poolPda,
        auditGrant: proj2GrantPda,
        grantVault: proj2VaultPda,
        usdcMint: usdcMint,
        funderTokenAccount: funderTokenAccount,
        admin: admin.publicKey,
        funder: funder.publicKey,
        applicant: applicant.publicKey,
        auditor: auditor.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        systemProgram: anchor.web3.SystemProgram.programId,
      })
      .signers([admin, funder])
      .rpc();

    // Admin cancels defaulted project
    const adminInitialBal = await getAccount(provider.connection, adminTokenAccount);
    await program.methods
      .cancelDefaultedGrant()
      .accounts({
        grantPool: poolPda,
        auditGrant: proj2GrantPda,
        grantVault: proj2VaultPda,
        adminTokenAccount: adminTokenAccount,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const adminFinalBal = await getAccount(provider.connection, adminTokenAccount);
    assert.equal(
      Number(adminFinalBal.amount) - Number(adminInitialBal.amount),
      4_000_000_000
    );

    const cancelledGrant = await program.account.auditGrant.fetch(proj2GrantPda);
    assert.isTrue("cancelled" in cancelledGrant.status);
  });
});