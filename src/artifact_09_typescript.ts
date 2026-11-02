import * as anchor from "@coral-xyz/anchor";
import { Program } from "@coral-xyz/anchor";
import { VaultlineGrantCore } from "../target/types/vaultline_grant_core";
import { PublicKey, Keypair } from "@solana/web3.js";
import {
  createMint,
  createAccount,
  mintTo,
  getAccount,
} from "@solana/spl-token";
import { expect } from "chai";
import * as crypto from "crypto";

describe("Vaultline Security Audit Grant Engine", () => {
  const provider = anchor.AnchorProvider.env();
  anchor.setProvider(provider);

  const program = anchor.workspace.VaultlineGrantCore as Program<VaultlineGrantCore>;

  let usdcMint: PublicKey;
  let admin: Keypair;
  let auditor: Keypair;
  let projectLead: Keypair;

  let adminTokenAccount: PublicKey;
  let auditorTokenAccount: PublicKey;

  let grantPda: PublicKey;
  let vaultTokenAccount: PublicKey;

  const projectId = "colosseum-renaissance-project-42";
  const dummyCommitHash = crypto.createHash("sha256").update("init-commit").digest();

  before(async () => {
    admin = Keypair.generate();
    auditor = Keypair.generate();
    projectLead = Keypair.generate();

    // Airdrop SOL
    const airdropSignatures = await Promise.all([
      provider.connection.requestAirdrop(admin.publicKey, 10 * anchor.web3.LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(auditor.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL),
      provider.connection.requestAirdrop(projectLead.publicKey, 2 * anchor.web3.LAMPORTS_PER_SOL),
    ]);
    for (const sig of airdropSignatures) {
      const latestBlockhash = await provider.connection.getLatestBlockhash();
      await provider.connection.confirmTransaction({ signature: sig, ...latestBlockhash });
    }

    // Create Mock USDC Mint (6 Decimals)
    usdcMint = await createMint(
      provider.connection,
      admin,
      admin.publicKey,
      null,
      6
    );

    // Create Token Accounts
    adminTokenAccount = await createAccount(
      provider.connection,
      admin,
      usdcMint,
      admin.publicKey
    );

    auditorTokenAccount = await createAccount(
      provider.connection,
      admin,
      usdcMint,
      auditor.publicKey
    );

    // Mint 10,000 USDC to Admin
    await mintTo(
      provider.connection,
      admin,
      usdcMint,
      adminTokenAccount,
      admin,
      10_000_000_000
    );

    [grantPda] = PublicKey.findProgramAddressSync(
      [Buffer.from("vaultline_grant"), admin.publicKey.toBuffer(), Buffer.from(projectId)],
      program.programId
    );

    [vaultTokenAccount] = PublicKey.findProgramAddressSync(
      [Buffer.from("grant_vault"), grantPda.toBuffer()],
      program.programId
    );
  });

  it("1. Initializes the Colosseum Project Grant Escrow (Up to 4,000 USDC)", async () => {
    await program.methods
      .initializeGrant(projectId, Array.from(dummyCommitHash))
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        usdcMint: usdcMint,
        auditor: auditor.publicKey,
        projectLead: projectLead.publicKey,
        admin: admin.publicKey,
        systemProgram: anchor.web3.SystemProgram.programId,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        rent: anchor.web3.SYSVAR_RENT_PUBKEY,
      })
      .signers([admin])
      .rpc();

    const grantAccount = await program.account.grantState.fetch(grantPda);
    expect(grantAccount.admin.toBase58()).to.equal(admin.publicKey.toBase58());
    expect(grantAccount.auditor.toBase58()).to.equal(auditor.publicKey.toBase58());
    expect(grantAccount.colosseumProjectId).to.equal(projectId);
    expect(grantAccount.totalFunded.toNumber()).to.equal(0);
    expect(grantAccount.totalDisbursed.toNumber()).to.equal(0);
  });

  it("2. Funds the Escrow with the full 4,000 USDC allocation", async () => {
    const fundAmount = new anchor.BN(4_000_000_000); // 4,000 USDC

    await program.methods
      .fundEscrow(fundAmount)
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        funderTokenAccount: adminTokenAccount,
        funder: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const vaultBalance = await getAccount(provider.connection, vaultTokenAccount);
    expect(Number(vaultBalance.amount)).to.equal(4_000_000_000);

    const grantAccount = await program.account.grantState.fetch(grantPda);
    expect(grantAccount.totalFunded.toNumber()).to.equal(4_000_000_000);
  });

  it("3. Prevents funding over the 4,000 USDC cap", async () => {
    try {
      const extraAmount = new anchor.BN(1_000_000);
      await program.methods
        .fundEscrow(extraAmount)
        .accounts({
          grantState: grantPda,
          vaultTokenAccount: vaultTokenAccount,
          funderTokenAccount: adminTokenAccount,
          funder: admin.publicKey,
          tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
        })
        .signers([admin])
        .rpc();
      expect.fail("Should have thrown ExceedsMaxGrantAmount");
    } catch (err: any) {
      expect(err.error.errorCode.code).to.equal("ExceedsMaxGrantAmount");
    }
  });

  it("4. Auditor submits Tranche 1 Scope artifact and Admin disburses 1,000 USDC", async () => {
    const scopeHash = crypto.createHash("sha256").update("scope-spec-doc-v1").digest();

    await program.methods
      .submitMilestoneArtifact({ scopeApproved: {} }, Array.from(scopeHash))
      .accounts({
        grantState: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    // Admin approves & disburses 1,000 USDC (Tranche 1)
    await program.methods
      .approveAndDisburseTranche({ scopeApproved: {} })
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        auditorTokenAccount: auditorTokenAccount,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    expect(Number(auditorBalance.amount)).to.equal(1_000_000_000); // 1,000 USDC
  });

  it("5. Auditor submits Tranche 2 Findings and Admin disburses 1,500 USDC", async () => {
    const findingsHash = crypto.createHash("sha256").update("preliminary-audit-report").digest();

    await program.methods
      .submitMilestoneArtifact({ findingsTriaged: {} }, Array.from(findingsHash))
      .accounts({
        grantState: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    // Admin approves & disburses 1,500 USDC (Tranche 2)
    await program.methods
      .approveAndDisburseTranche({ findingsTriaged: {} })
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        auditorTokenAccount: auditorTokenAccount,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    expect(Number(auditorBalance.amount)).to.equal(2_500_000_000); // 2,500 USDC cumulative
  });

  it("6. Auditor submits Tranche 3 Final Remediation and receives final 1,500 USDC", async () => {
    const finalReportHash = crypto.createHash("sha256").update("final-clean-audit-signoff").digest();

    await program.methods
      .submitMilestoneArtifact({ remediationsSignedOff: {} }, Array.from(finalReportHash))
      .accounts({
        grantState: grantPda,
        auditor: auditor.publicKey,
      })
      .signers([auditor])
      .rpc();

    // Admin approves & disburses final 1,500 USDC (Tranche 3)
    await program.methods
      .approveAndDisburseTranche({ remediationsSignedOff: {} })
      .accounts({
        grantState: grantPda,
        vaultTokenAccount: vaultTokenAccount,
        auditorTokenAccount: auditorTokenAccount,
        admin: admin.publicKey,
        tokenProgram: anchor.utils.token.TOKEN_PROGRAM_ID,
      })
      .signers([admin])
      .rpc();

    const auditorBalance = await getAccount(provider.connection, auditorTokenAccount);
    expect(Number(auditorBalance.amount)).to.equal(4_000_000_000); // 4,000 USDC total

    const vaultBalance = await getAccount(provider.connection, vaultTokenAccount);
    expect(Number(vaultBalance.amount)).to.equal(0); // Fully depleted
  });
});