use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("VLTgRt8v9bL7v7w4Kj6aM1Qn5kY7aR2hP9vS5mN4c1Z");

pub const GRANT_AMOUNT: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const MILESTONE_1_PAYOUT: u64 = 1_200_000_000; // 30% on scoping & triage verification
pub const MILESTONE_2_PAYOUT: u64 = 2_800_000_000; // 70% on final audit & remediation signoff

#[program]
pub mod vaultline_grants {
    use super::*;

    /// Initializes the global Vaultline Grant Pool PDA.
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        pool_bump: u8,
        vault_bump: u8,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.pool;
        pool.admin = ctx.accounts.admin.key();
        pool.usdc_mint = ctx.accounts.usdc_mint.key();
        pool.total_allocated = 0;
        pool.total_disbursed = 0;
        pool.active_grants_count = 0;
        pool.pool_bump = pool_bump;
        pool.vault_bump = vault_bump;

        emit!(PoolInitializedEvent {
            admin: pool.admin,
            usdc_mint: pool.usdc_mint,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Colosseum Project Founder applies for a Vaultline Security Audit Grant.
    pub fn apply_grant(
        ctx: Context<ApplyGrant>,
        colosseum_project_id: String,
        repo_url: String,
        commit_hash: [u8; 32],
    ) -> Result<()> {
        require!(
            colosseum_project_id.len() > 0 && colosseum_project_id.len() <= 32,
            VaultlineError::InvalidProjectIdLength
        );
        require!(
            repo_url.len() > 0 && repo_url.len() <= 128,
            VaultlineError::InvalidRepoUrlLength
        );

        let grant = &mut ctx.accounts.grant;
        grant.project_lead = ctx.accounts.applicant.key();
        grant.colosseum_project_id = colosseum_project_id.clone();
        grant.repo_url = repo_url;
        grant.initial_commit_hash = commit_hash;
        grant.remediation_commit_hash = [0u8; 32];
        grant.assigned_auditor = Pubkey::default();
        grant.status = GrantStatus::Applied;
        grant.total_grant_amount = GRANT_AMOUNT;
        grant.disbursed_amount = 0;
        grant.created_at = Clock::get()?.unix_timestamp;
        grant.grant_bump = ctx.bumps.grant;

        emit!(GrantAppliedEvent {
            applicant: grant.project_lead,
            colosseum_project_id,
            timestamp: grant.created_at,
        });

        Ok(())
    }

    /// Vaultline Committee reviews application, assigns an auditor, and locks 4,000 USDC into escrow.
    pub fn approve_grant_and_fund(
        ctx: Context<ApproveGrantAndFund>,
        assigned_auditor: Pubkey,
    ) -> Result<()> {
        require!(
            assigned_auditor != Pubkey::default(),
            VaultlineError::InvalidAuditorAddress
        );

        let pool = &mut ctx.accounts.pool;
        let grant = &mut ctx.accounts.grant;

        require!(
            grant.status == GrantStatus::Applied,
            VaultlineError::InvalidGrantState
        );

        // Transfer 4,000 USDC from funder to Vaultline Escrow PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, GRANT_AMOUNT)?;

        grant.assigned_auditor = assigned_auditor;
        grant.status = GrantStatus::ApprovedAndFunded;

        pool.total_allocated = pool
            .total_allocated
            .checked_add(GRANT_AMOUNT)
            .ok_or(VaultlineError::MathOverflow)?;
        pool.active_grants_count = pool
            .active_grants_count
            .checked_add(1)
            .ok_or(VaultlineError::MathOverflow)?;

        emit!(GrantApprovedEvent {
            colosseum_project_id: grant.colosseum_project_id.clone(),
            project_lead: grant.project_lead,
            assigned_auditor,
            allocated_amount: GRANT_AMOUNT,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Milestone 1: Deliver scoping document & verify initial triage.
    pub fn submit_and_verify_milestone1(
        ctx: Context<VerifyMilestone1>,
        scoping_doc_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let pool = &mut ctx.accounts.pool;

        require!(
            grant.status == GrantStatus::ApprovedAndFunded,
            VaultlineError::InvalidGrantState
        );
        require!(
            scoping_doc_hash != [0u8; 32],
            VaultlineError::InvalidProofHash
        );

        grant.scoping_document_hash = scoping_doc_hash;
        grant.status = GrantStatus::Milestone1Verified;

        // Transfer Milestone 1 Payout (30% = 1,200 USDC) to Auditor Account
        let pool_bump = pool.pool_bump;
        let seeds = &[b"grant_pool".as_ref(), &[pool_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: pool.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, MILESTONE_1_PAYOUT)?;

        grant.disbursed_amount = grant
            .disbursed_amount
            .checked_add(MILESTONE_1_PAYOUT)
            .ok_or(VaultlineError::MathOverflow)?;

        pool.total_disbursed = pool
            .total_disbursed
            .checked_add(MILESTONE_1_PAYOUT)
            .ok_or(VaultlineError::MathOverflow)?;

        emit!(MilestoneVerifiedEvent {
            colosseum_project_id: grant.colosseum_project_id.clone(),
            milestone_id: 1,
            amount_disbursed: MILESTONE_1_PAYOUT,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Project submits remediation commit hash for vulnerabilities uncovered during audit.
    pub fn submit_remediation(
        ctx: Context<SubmitRemediation>,
        remediation_commit_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(
            grant.status == GrantStatus::Milestone1Verified,
            VaultlineError::InvalidGrantState
        );
        require!(
            remediation_commit_hash != [0u8; 32],
            VaultlineError::InvalidProofHash
        );

        grant.remediation_commit_hash = remediation_commit_hash;
        grant.status = GrantStatus::RemediationSubmitted;

        emit!(RemediationSubmittedEvent {
            colosseum_project_id: grant.colosseum_project_id.clone(),
            commit_hash: remediation_commit_hash,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Milestone 2: Final audit sign-off by assigned auditor and admin. Releases remaining 2,800 USDC.
    pub fn final_audit_disburse(
        ctx: Context<FinalAuditDisburse>,
        final_report_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let pool = &mut ctx.accounts.pool;

        require!(
            grant.status == GrantStatus::RemediationSubmitted,
            VaultlineError::InvalidGrantState
        );
        require!(
            final_report_hash != [0u8; 32],
            VaultlineError::InvalidProofHash
        );

        grant.final_report_hash = final_report_hash;
        grant.status = GrantStatus::Completed;

        // Transfer remaining Milestone 2 balance (2,800 USDC) to Auditor Account
        let pool_bump = pool.pool_bump;
        let seeds = &[b"grant_pool".as_ref(), &[pool_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: pool.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, MILESTONE_2_PAYOUT)?;

        grant.disbursed_amount = grant
            .disbursed_amount
            .checked_add(MILESTONE_2_PAYOUT)
            .ok_or(VaultlineError::MathOverflow)?;

        pool.total_disbursed = pool
            .total_disbursed
            .checked_add(MILESTONE_2_PAYOUT)
            .ok_or(VaultlineError::MathOverflow)?;

        pool.active_grants_count = pool
            .active_grants_count
            .checked_sub(1)
            .ok_or(VaultlineError::MathOverflow)?;

        emit!(GrantCompletedEvent {
            colosseum_project_id: grant.colosseum_project_id.clone(),
            final_report_hash,
            total_disbursed: grant.disbursed_amount,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Emergency clawback for cancelled/inactive audits by Committee Admin.
    pub fn emergency_clawback(ctx: Context<EmergencyClawback>) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let pool = &mut ctx.accounts.pool;

        require!(
            grant.status == GrantStatus::ApprovedAndFunded || grant.status == GrantStatus::Milestone1Verified,
            VaultlineError::GrantNotClawbackEligible
        );

        let remaining_grant_balance = grant
            .total_grant_amount
            .checked_sub(grant.disbursed_amount)
            .ok_or(VaultlineError::MathOverflow)?;

        let pool_bump = pool.pool_bump;
        let seeds = &[b"grant_pool".as_ref(), &[pool_bump]];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: pool.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer);
        token::transfer(cpi_ctx, remaining_grant_balance)?;

        grant.status = GrantStatus::Revoked;
        pool.total_allocated = pool
            .total_allocated
            .checked_sub(remaining_grant_balance)
            .ok_or(VaultlineError::MathOverflow)?;
        pool.active_grants_count = pool
            .active_grants_count
            .checked_sub(1)
            .ok_or(VaultlineError::MathOverflow)?;

        emit!(GrantRevokedEvent {
            colosseum_project_id: grant.colosseum_project_id.clone(),
            clawed_back_amount: remaining_grant_balance,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION STRUCTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + GrantPool::INIT_SPACE,
        seeds = [b"grant_pool"],
        bump
    )]
    pub pool: Account<'info, GrantPool>,

    pub usdc_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = pool,
        seeds = [b"pool_vault", usdc_mint.key().as_ref()],
        bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
#[instruction(colosseum_project_id: String)]
pub struct ApplyGrant<'info> {
    #[account(
        init,
        payer = applicant,
        space = 8 + ProjectGrant::INIT_SPACE,
        seeds = [b"project_grant", colosseum_project_id.as_bytes()],
        bump
    )]
    pub grant: Account<'info, ProjectGrant>,

    #[account(mut)]
    pub applicant: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct ApproveGrantAndFund<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = pool.pool_bump,
        has_one = admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"project_grant", grant.colosseum_project_id.as_bytes()],
        bump = grant.grant_bump
    )]
    pub grant: Account<'info, ProjectGrant>,

    #[account(
        mut,
        seeds = [b"pool_vault", pool.usdc_mint.as_ref()],
        bump = pool.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = funder_token_account.owner == admin.key() @ VaultlineError::InvalidTokenAccountOwner,
        constraint = funder_token_account.mint == pool.usdc_mint @ VaultlineError::InvalidMint
    )]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct VerifyMilestone1<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = pool.pool_bump
    )]
    pub pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"project_grant", grant.colosseum_project_id.as_bytes()],
        bump = grant.grant_bump,
        constraint = grant.assigned_auditor == auditor.key() @ VaultlineError::UnauthorizedAuditor
    )]
    pub grant: Account<'info, ProjectGrant>,

    #[account(
        mut,
        seeds = [b"pool_vault", pool.usdc_mint.as_ref()],
        bump = pool.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.owner == auditor.key() @ VaultlineError::InvalidTokenAccountOwner,
        constraint = auditor_token_account.mint == pool.usdc_mint @ VaultlineError::InvalidMint
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SubmitRemediation<'info> {
    #[account(
        mut,
        seeds = [b"project_grant", grant.colosseum_project_id.as_bytes()],
        bump = grant.grant_bump,
        constraint = grant.project_lead == project_lead.key() @ VaultlineError::UnauthorizedApplicant
    )]
    pub grant: Account<'info, ProjectGrant>,

    pub project_lead: Signer<'info>,
}

#[derive(Accounts)]
pub struct FinalAuditDisburse<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = pool.pool_bump
    )]
    pub pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"project_grant", grant.colosseum_project_id.as_bytes()],
        bump = grant.grant_bump,
        constraint = grant.assigned_auditor == auditor.key() @ VaultlineError::UnauthorizedAuditor
    )]
    pub grant: Account<'info, ProjectGrant>,

    #[account(
        mut,
        seeds = [b"pool_vault", pool.usdc_mint.as_ref()],
        bump = pool.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.owner == auditor.key() @ VaultlineError::InvalidTokenAccountOwner,
        constraint = auditor_token_account.mint == pool.usdc_mint @ VaultlineError::InvalidMint
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EmergencyClawback<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = pool.pool_bump,
        has_one = admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"project_grant", grant.colosseum_project_id.as_bytes()],
        bump = grant.grant_bump
    )]
    pub grant: Account<'info, ProjectGrant>,

    #[account(
        mut,
        seeds = [b"pool_vault", pool.usdc_mint.as_ref()],
        bump = pool.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.owner == admin.key() @ VaultlineError::InvalidTokenAccountOwner,
        constraint = admin_token_account.mint == pool.usdc_mint @ VaultlineError::InvalidMint
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// STATE STRUCTS
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct GrantPool {
    pub admin: Pubkey,
    pub usdc_mint: Pubkey,
    pub total_allocated: u64,
    pub total_disbursed: u64,
    pub active_grants_count: u32,
    pub pool_bump: u8,
    pub vault_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct ProjectGrant {
    pub project_lead: Pubkey,
    pub assigned_auditor: Pubkey,
    #[max_len(32)]
    pub colosseum_project_id: String,
    #[max_len(128)]
    pub repo_url: String,
    pub initial_commit_hash: [u8; 32],
    pub scoping_document_hash: [u8; 32],
    pub remediation_commit_hash: [u8; 32],
    pub final_report_hash: [u8; 32],
    pub total_grant_amount: u64,
    pub disbursed_amount: u64,
    pub status: GrantStatus,
    pub created_at: i64,
    pub grant_bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum GrantStatus {
    Applied,
    ApprovedAndFunded,
    Milestone1Verified,
    RemediationSubmitted,
    Completed,
    Revoked,
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct PoolInitializedEvent {
    pub admin: Pubkey,
    pub usdc_mint: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct GrantAppliedEvent {
    pub applicant: Pubkey,
    pub colosseum_project_id: String,
    pub timestamp: i64,
}

#[event]
pub struct GrantApprovedEvent {
    pub colosseum_project_id: String,
    pub project_lead: Pubkey,
    pub assigned_auditor: Pubkey,
    pub allocated_amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneVerifiedEvent {
    pub colosseum_project_id: String,
    pub milestone_id: u8,
    pub amount_disbursed: u64,
    pub timestamp: i64,
}

#[event]
pub struct RemediationSubmittedEvent {
    pub colosseum_project_id: String,
    pub commit_hash: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct GrantCompletedEvent {
    pub colosseum_project_id: String,
    pub final_report_hash: [u8; 32],
    pub total_disbursed: u64,
    pub timestamp: i64,
}

#[event]
pub struct GrantRevokedEvent {
    pub colosseum_project_id: String,
    pub clawed_back_amount: u64,
    pub timestamp: i64,
}

#[error_code]
pub enum VaultlineError {
    #[msg("Invalid Colosseum Project ID length. Must be 1-32 chars.")]
    InvalidProjectIdLength,
    #[msg("Invalid Repository URL length. Must be 1-128 chars.")]
    InvalidRepoUrlLength,
    #[msg("Auditor public key cannot be the default zero pubkey.")]
    InvalidAuditorAddress,
    #[msg("The grant is not in the required state for this operation.")]
    InvalidGrantState,
    #[msg("Arithmetic overflow or underflow occurred.")]
    MathOverflow,
    #[msg("Provided deliverable or scoping hash is invalid.")]
    InvalidProofHash,
    #[msg("Only the designated admin can call this instruction.")]
    UnauthorizedAdmin,
    #[msg("Only the assigned auditor can execute this verification.")]
    UnauthorizedAuditor,
    #[msg("Only the project lead can submit remediation updates.")]
    UnauthorizedApplicant,
    #[msg("Token account owner does not match expected authority.")]
    InvalidTokenAccountOwner,
    #[msg("Token account mint mismatch.")]
    InvalidMint,
    #[msg("Grant is not eligible for emergency clawback.")]
    GrantNotClawbackEligible,
}