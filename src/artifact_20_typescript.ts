import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { VaultlineGrants } from "../target/types/vaultline_grants";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
  TOKEN_PROGRAM_ID,
} from "@solana/spl-token";
import { Keypair, PublicKey, SystemProgram, SYSVAR_RENT_PUBKEY } from "@solana/web3.js";
import { expect } from "chai";
import * as crypto from "crypto";

describe("Vaultline Security Audit Grants Protocol QA Suite", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.VaultlineGrants as Program<VaultlineGrants>;

  const admin = Keypair.generate();
  const grantee = Keypair.generate();
  const auditor = Keypair.generate();
  const unauthorizedActor = Keypair.generate();

  let usdcMint: PublicKey;
  let adminUsdcAccount: PublicKey;
  let auditorUsdcAccount: PublicKey;

  const grantId = crypto.randomBytes(16);
  const grantAmount = new anchor.BN(4_000_000_000); // 4,000 USDC
  const slaSeconds = new anchor.BN(86400 * 14);     // 14 day SLA

  let configPda: PublicKey;
  let grantPda: PublicKey;
  let escrowVaultPda: PublicKey;

  before(async () => {
    // 1. Airdrop SOL to test actors
    for (const actor of [admin, grantee, auditor, unauthorizedActor]) {
      const sig = await provider.connection.requestAirdrop(
        actor.publicKey,
        10 * anchor.web3.LAMPORTS_PER_SOL
      );
      await provider.connection.confirmTransaction(sig);
    }

    // 2. Initialize test USDC mint (6 decimals)
    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6
    );

    // 3. Create associated token accounts
    adminUsdcAccount = await createAccount(
      provider.connection,
      admin,
      usdcMint,
      admin.publicKey
    );

    auditorUsdcAccount = await createAccount(
      provider.connection,
      auditor,
      usdcMint,
      auditor.publicKey
    );

    // 4. Mint 10,000 USDC to admin
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      adminUsdcAccount,
      admin,
      10_000_000_000
    );

    // Compute PDAs
    [configPda] = PublicKey.findProgramAddressSync([Buffer.from("config")], program.programId);
    [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), grantId],
      program.programId
    );
    [escrowVaultPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), grantPda.toBuffer()],
      program.programId
    );
  });

  it("1. Initializes global program config", async () => {
    await program.methods
      .initialize()
      .accounts({
        config: configPda,
        usdcMint: usdcMint,
        admin: admin.publicKey,
        systemProgram: SystemProgram.programId,
      })
      .signers([admin])
      .rpc();

    const configAccount = await program.account.programConfig.fetch(configPda);
    expect(configAccount.admin.toBase58()).to.equal(admin.publicKey.toBase58());
    expect(configAccount.usdcMint.toBase58()).to.equal(usdcMint.toBase58());
    expect(configAccount.totalGrantsCreated.toNumber()).to.equal(0);
  });

  it("2. Rejects grant creation exceeding the 4,000 USDC maximum cap", async () => {
    const invalidAmount = new anchor.BN(4_001_000_000); // 4,001 USDC
    const fakeGrantId = crypto.randomBytes(16);
    const [fakeGrant] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant"), fakeGrantId],
      program.programId
    );
    const [fakeVault] = PublicKey.findProgramAddressSync(
      [Buffer.from("escrow"), fakeGrant.toBuffer()],
      program.programId
    );

    try {
      await program.methods
        .applyGrant(Array.from(fakeGrantId), invalidAmount, slaSeconds)
        .accounts({
          config: configPda,
          grant: fakeGrant,
          escrowVault: fakeVault,
          usdcMint: usdcMint,
          granteeAuthority: grantee.publicKey,
          auditor: auditor.publicKey,
          payer: admin.publicKey,
          systemProgram: SystemProgram.programId,
          tokenProgram: TOKEN_PROGRAM_ID,
          rent: SYSVAR_RENT_PUBKEY,
        })
        .signers([admin])
        .rpc();
      expect.fail("Should have failed with ExceedsMaxGrantCap");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("ExceedsMaxGrantCap");
    }
  });

  it("3. Successfully registers a valid 4,000 USDC grant", async () => {
    await program.methods
      .applyGrant(Array.from(grantId), grantAmount, slaSeconds)
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        usdcMint: usdcMint,
        granteeAuthority: grantee.publicKey,
        auditor: auditor.publicKey,
        payer: admin.publicKey,
        systemProgram: SystemProgram.programId,
        tokenProgram: TOKEN_PROGRAM_ID,
        rent: SYSVAR_RENT_PUBKEY,
      })
      .signers([admin])
      .rpc();

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    expect(grantAccount.totalAmount.toNumber()).to.equal(4_000_000_000);
    expect(grantAccount.milestone1Amount.toNumber()).to.equal(2_800_000_000); // 70%
    expect(grantAccount.milestone2Amount.toNumber()).to.equal(1_200_000_000); // 30%
    expect(grantAccount.milestone1Paid).to.be.false;
    expect(grantAccount.milestone2Paid).to.be.false;
  });

  it("4. Funds the escrow vault with 4,000 USDC", async () => {
    await program.methods
      .fundGrant()
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        funderTokenAccount: adminUsdcAccount,
        usdcMint: usdcMint,
        funder: admin.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const vaultToken = await getAccount(provider.connection, escrowVaultPda);
    expect(Number(vaultToken.amount)).to.equal(4_000_000_000);
  });

  it("5. Rejects unauthorized report submission", async () => {
    const dummyHash = crypto.randomBytes(32);

    try {
      await program.methods
        .submitReport(Array.from(dummyHash))
        .accounts({
          grant: grantPda,
          auditor: unauthorizedActor.publicKey,
        })
        .signers([unauthorizedActor])
        .rpc();
      expect.fail("Should have failed with constraint violation or unauthorized signer");
    } catch (err: any) {
      expect(err.toString()).to.include("ConstraintHasOne");
    }
  });

  it("6. Submits legitimate audit report digest by certified auditor", async () => {
    const reportDigest = crypto.createHash("sha256").update("Vaultline Audit Report v1.0").digest();

    await program.methods
      .submitReport(Array.from(reportDigest))
      .accounts({
        grant: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    expect(Buffer.from(grantAccount.reportDigest).toString("hex")).to.equal(reportDigest.toString("hex"));
  });

  it("7. Releases Milestone 1 (2,800 USDC / 70%) to auditor", async () => {
    const initialAuditorBal = (await getAccount(provider.connection, auditorUsdcAccount)).amount;

    await program.methods
      .releaseMilestone1()
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        auditorTokenAccount: auditorUsdcAccount,
        usdcMint: usdcMint,
        signer: grantee.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([grantee])
      .rpc();

    const finalAuditorBal = (await getAccount(provider.connection, auditorUsdcAccount)).amount;
    expect(Number(finalAuditorBal - initialAuditorBal)).to.equal(2_800_000_000);

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    expect(grantAccount.milestone1Paid).to.be.true;
  });

  it("8. Releases Milestone 2 (1,200 USDC / 30%) upon remediation co-sign", async () => {
    const remediationDigest = crypto.createHash("sha256").update("Remediation Patch Signoff").digest();
    const initialAuditorBal = (await getAccount(provider.connection, auditorUsdcAccount)).amount;

    await program.methods
      .releaseMilestone2(Array.from(remediationDigest))
      .accounts({
        config: configPda,
        grant: grantPda,
        escrowVault: escrowVaultPda,
        auditorTokenAccount: auditorUsdcAccount,
        usdcMint: usdcMint,
        signer: auditor.publicKey,
        tokenProgram: TOKEN_PROGRAM_ID,
      })
      .signers([auditor])
      .rpc();

    const finalAuditorBal = (await getAccount(provider.connection, auditorUsdcAccount)).amount;
    expect(Number(finalAuditorBal - initialAuditorBal)).to.equal(1_200_000_000);

    const vaultToken = await getAccount(provider.connection, escrowVaultPda);
    expect(Number(vaultToken.amount)).to.equal(0); // Fully settled

    const grantAccount = await program.account.auditGrant.fetch(grantPda);
    expect(grantAccount.milestone2Paid).to.be.true;
  });
});