use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("Vau1tXkFfR72b1Wc8W5m1k9wH6F5uB6A4d2S1q8pY9Z");

pub const GRANT_SEED: &[u8] = b"grant";
pub const VAULT_SEED: &[u8] = b"vault";
pub const MAX_GRANT_AMOUNT: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const MILESTONE_PERCENTAGE: u64 = 50; // 50% split per milestone

#[program]
pub mod vaultline_grant_protocol {
    use super::*;

    /// Initialize a new Colosseum project grant application
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        project_id: String,
        project_name: String,
        repo_hash: [u8; 32],
        grant_amount: u64,
    ) -> Result<()> {
        require!(
            grant_amount > 0 && grant_amount <= MAX_GRANT_AMOUNT,
            VaultlineError::InvalidGrantAmount
        );
        require!(
            project_id.len() > 0 && project_id.len() <= 32,
            VaultlineError::InvalidProjectId
        );
        require!(
            project_name.len() > 0 && project_name.len() <= 64,
            VaultlineError::InvalidProjectName
        );

        let grant = &mut ctx.accounts.grant;
        grant.authority = ctx.accounts.authority.key();
        grant.vaultline_admin = ctx.accounts.vaultline_admin.key();
        grant.project_recipient = ctx.accounts.project_recipient.key();
        grant.auditor = Pubkey::default();
        grant.mint = ctx.accounts.mint.key();
        grant.project_id = project_id.clone();
        grant.project_name = project_name;
        grant.repo_hash = repo_hash;
        grant.grant_amount = grant_amount;
        grant.disbursed_amount = 0;
        grant.status = GrantStatus::PendingApproval;
        grant.bump = ctx.bumps.grant;
        grant.vault_bump = ctx.bumps.vault_account;
        grant.created_at = Clock::get()?.unix_timestamp;

        emit!(GrantInitializedEvent {
            project_id,
            grant_pda: grant.key(),
            recipient: grant.project_recipient,
            amount: grant_amount,
            timestamp: grant.created_at,
        });

        Ok(())
    }

    /// Admin approves the grant, binds an approved auditor, and deposits USDC into escrow
    pub fn approve_and_fund_grant(
        ctx: Context<ApproveAndFundGrant>,
        auditor: Pubkey,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(
            grant.status == GrantStatus::PendingApproval,
            VaultlineError::InvalidStateTransition
        );
        require!(auditor != Pubkey::default(), VaultlineError::InvalidAuditorKey);

        grant.auditor = auditor;
        grant.status = GrantStatus::ApprovedAndFunded;

        // Transfer grant_amount USDC from admin ATA to Escrow Vault PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.admin_token_account.to_account_info(),
            to: ctx.accounts.vault_account.to_account_info(),
            authority: ctx.accounts.vaultline_admin.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, grant.grant_amount)?;

        emit!(GrantFundedEvent {
            project_id: grant.project_id.clone(),
            auditor,
            funded_amount: grant.grant_amount,
        });

        Ok(())
    }

    /// Auditor signs off on the preliminary audit report (Milestone 1: 50% payout)
    pub fn submit_audit_attestation(
        ctx: Context<SubmitAttestation>,
        report_cid: [u8; 32],
        critical_vulnerabilities: u16,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(
            grant.status == GrantStatus::ApprovedAndFunded,
            VaultlineError::InvalidStateTransition
        );
        require_keys_eq!(
            ctx.accounts.auditor.key(),
            grant.auditor,
            VaultlineError::UnauthorizedAuditor
        );

        grant.report_cid = report_cid;
        grant.critical_vulnerabilities = critical_vulnerabilities;
        grant.status = GrantStatus::AuditCompleted;

        // Disburse 50% to auditor
        let payout = (grant.grant_amount * MILESTONE_PERCENTAGE) / 100;
        grant.disbursed_amount = grant.disbursed_amount.checked_add(payout).unwrap();

        let project_id_bytes = grant.project_id.as_bytes();
        let authority_key = grant.authority.as_ref();
        let bump = grant.bump;
        let seeds: &[&[&[u8]]] = &[&[
            GRANT_SEED,
            project_id_bytes,
            authority_key,
            &[bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds);
        token::transfer(cpi_ctx, payout)?;

        emit!(MilestoneAttestedEvent {
            project_id: grant.project_id.clone(),
            stage: MilestoneStage::AuditCompleted,
            report_cid,
            disbursed_to: ctx.accounts.auditor.key(),
            disbursed_amount: payout,
        });

        Ok(())
    }

    /// Final Remediation sign-off: Disburses remaining 50% (split or paid to project/auditor)
    pub fn finalize_remediation(
        ctx: Context<FinalizeRemediation>,
        remediation_commit_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(
            grant.status == GrantStatus::AuditCompleted,
            VaultlineError::InvalidStateTransition
        );
        require_keys_eq!(
            ctx.accounts.auditor.key(),
            grant.auditor,
            VaultlineError::UnauthorizedAuditor
        );

        grant.remediation_commit_hash = remediation_commit_hash;
        grant.status = GrantStatus::RemediationVerified;

        // Remaining balance disbursement
        let remaining_payout = grant.grant_amount.checked_sub(grant.disbursed_amount).unwrap();
        grant.disbursed_amount = grant.disbursed_amount.checked_add(remaining_payout).unwrap();

        let project_id_bytes = grant.project_id.as_bytes();
        let authority_key = grant.authority.as_ref();
        let bump = grant.bump;
        let seeds: &[&[&[u8]]] = &[&[
            GRANT_SEED,
            project_id_bytes,
            authority_key,
            &[bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.project_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds);
        token::transfer(cpi_ctx, remaining_payout)?;

        emit!(GrantCompletedEvent {
            project_id: grant.project_id.clone(),
            final_payout: remaining_payout,
            recipient: grant.project_recipient,
        });

        Ok(())
    }

    /// Emergency cancellation by Vaultline Admin prior to full audit execution
    pub fn emergency_refund(ctx: Context<EmergencyRefund>) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(
            grant.status != GrantStatus::RemediationVerified,
            VaultlineError::GrantAlreadyCompleted
        );

        let remaining = ctx.accounts.vault_account.amount;
        require!(remaining > 0, VaultlineError::ZeroVaultBalance);

        let project_id_bytes = grant.project_id.as_bytes();
        let authority_key = grant.authority.as_ref();
        let bump = grant.bump;
        let seeds: &[&[&[u8]]] = &[&[
            GRANT_SEED,
            project_id_bytes,
            authority_key,
            &[bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_account.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds);
        token::transfer(cpi_ctx, remaining)?;

        grant.status = GrantStatus::EmergencyRevoked;

        emit!(EmergencyRefundEvent {
            project_id: grant.project_id.clone(),
            refunded_amount: remaining,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(project_id: String)]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = authority,
        space = GrantAccount::LEN,
        seeds = [GRANT_SEED, project_id.as_bytes(), authority.key().as_ref()],
        bump
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        init,
        payer = authority,
        seeds = [VAULT_SEED, grant.key().as_ref()],
        bump,
        token::mint = mint,
        token::authority = grant
    )]
    pub vault_account: Account<'info, TokenAccount>,

    pub mint: Account<'info, Mint>,

    /// CHECK: Recipient public key for verified payouts
    pub project_recipient: AccountInfo<'info>,

    /// CHECK: Vaultline administrative controller
    pub vaultline_admin: AccountInfo<'info>,

    #[account(mut)]
    pub authority: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ApproveAndFundGrant<'info> {
    #[account(
        mut,
        has_one = vaultline_admin @ VaultlineError::UnauthorizedAdmin,
        seeds = [GRANT_SEED, grant.project_id.as_bytes(), grant.authority.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.mint == grant.mint @ VaultlineError::InvalidMint,
        constraint = admin_token_account.owner == vaultline_admin.key() @ VaultlineError::InvalidTokenOwner
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub vaultline_admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SubmitAttestation<'info> {
    #[account(
        mut,
        seeds = [GRANT_SEED, grant.project_id.as_bytes(), grant.authority.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.mint == grant.mint @ VaultlineError::InvalidMint,
        constraint = auditor_token_account.owner == grant.auditor @ VaultlineError::InvalidTokenOwner
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FinalizeRemediation<'info> {
    #[account(
        mut,
        seeds = [GRANT_SEED, grant.project_id.as_bytes(), grant.authority.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = project_token_account.mint == grant.mint @ VaultlineError::InvalidMint,
        constraint = project_token_account.owner == grant.project_recipient @ VaultlineError::InvalidTokenOwner
    )]
    pub project_token_account: Account<'info, TokenAccount>,

    pub auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EmergencyRefund<'info> {
    #[account(
        mut,
        has_one = vaultline_admin @ VaultlineError::UnauthorizedAdmin,
        seeds = [GRANT_SEED, grant.project_id.as_bytes(), grant.authority.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [VAULT_SEED, grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub vault_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.mint == grant.mint @ VaultlineError::InvalidMint,
        constraint = admin_token_account.owner == vaultline_admin.key() @ VaultlineError::InvalidTokenOwner
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub vaultline_admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// STATE AND ERROR ENUMS
// -----------------------------------------------------------------------------

#[account]
pub struct GrantAccount {
    pub authority: Pubkey,
    pub vaultline_admin: Pubkey,
    pub project_recipient: Pubkey,
    pub auditor: Pubkey,
    pub mint: Pubkey,
    pub project_id: String,
    pub project_name: String,
    pub repo_hash: [u8; 32],
    pub report_cid: [u8; 32],
    pub remediation_commit_hash: [u8; 32],
    pub grant_amount: u64,
    pub disbursed_amount: u64,
    pub critical_vulnerabilities: u16,
    pub status: GrantStatus,
    pub bump: u8,
    pub vault_bump: u8,
    pub created_at: i64,
}

impl GrantAccount {
    pub const LEN: usize = 8 + // discriminator
        32 + // authority
        32 + // vaultline_admin
        32 + // project_recipient
        32 + // auditor
        32 + // mint
        (4 + 32) + // project_id
        (4 + 64) + // project_name
        32 + // repo_hash
        32 + // report_cid
        32 + // remediation_commit_hash
        8  + // grant_amount
        8  + // disbursed_amount
        2  + // critical_vulnerabilities
        1  + // status enum
        1  + // bump
        1  + // vault_bump
        8;   // created_at
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum GrantStatus {
    PendingApproval,
    ApprovedAndFunded,
    AuditCompleted,
    RemediationVerified,
    EmergencyRevoked,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, Eq)]
pub enum MilestoneStage {
    AuditCompleted,
    RemediationVerified,
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub project_id: String,
    pub grant_pda: Pubkey,
    pub recipient: Pubkey,
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct GrantFundedEvent {
    pub project_id: String,
    pub auditor: Pubkey,
    pub funded_amount: u64,
}

#[event]
pub struct MilestoneAttestedEvent {
    pub project_id: String,
    pub stage: MilestoneStage,
    pub report_cid: [u8; 32],
    pub disbursed_to: Pubkey,
    pub disbursed_amount: u64,
}

#[event]
pub struct GrantCompletedEvent {
    pub project_id: String,
    pub final_payout: u64,
    pub recipient: Pubkey,
}

#[event]
pub struct EmergencyRefundEvent {
    pub project_id: String,
    pub refunded_amount: u64,
}

// -----------------------------------------------------------------------------
// ERRORS
// -----------------------------------------------------------------------------

#[error_code]
pub enum VaultlineError {
    #[msg("Grant amount exceeds maximum ceiling of 4,000 USDC.")]
    InvalidGrantAmount,
    #[msg("Invalid project identifier length.")]
    InvalidProjectId,
    #[msg("Invalid project name length.")]
    InvalidProjectName,
    #[msg("Invalid state transition for this action.")]
    InvalidStateTransition,
    #[msg("Invalid auditor public key.")]
    InvalidAuditorKey,
    #[msg("Unauthorized Vaultline Admin.")]
    UnauthorizedAdmin,
    #[msg("Signer is not the authorized auditor.")]
    UnauthorizedAuditor,
    #[msg("Token mint mismatch.")]
    InvalidMint,
    #[msg("Invalid Token Account Owner.")]
    InvalidTokenOwner,
    #[msg("Grant is already completed.")]
    GrantAlreadyCompleted,
    #[msg("Vault balance is zero.")]
    ZeroVaultBalance,
}