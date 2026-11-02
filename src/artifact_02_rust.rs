use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("V1tLinEZ7sQ72cRBN5fMvY5oQoK4wMUPe3sFmKzJj9e");

pub const GRANT_TOTAL_AMOUNT: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const MILESTONE_1_AMOUNT: u64 = 1_000_000_000; // 25% (1,000 USDC)
pub const MILESTONE_2_AMOUNT: u64 = 2_000_000_000; // 50% (2,000 USDC)
pub const MILESTONE_3_AMOUNT: u64 = 1_000_000_000; // 25% (1,000 USDC)

#[program]
pub mod vaultline_sentinel {
    use super::*;

    /// Initializes a new security audit grant for a Colosseum project.
    /// Deposits 4,000 USDC into the program's vault PDA.
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        project_id: String,
        repo_commit_hash: [u8; 32],
    ) -> Result<()> {
        require!(project_id.len() > 0 && project_id.len() <= 64, VaultlineError::InvalidProjectIdLength);
        
        let grant = &mut ctx.accounts.grant_account;
        grant.admin = ctx.accounts.admin.key();
        grant.auditor = ctx.accounts.auditor.key();
        grant.developer = ctx.accounts.developer.key();
        grant.usdc_mint = ctx.accounts.usdc_mint.key();
        grant.vault_token_account = ctx.accounts.vault_token_account.key();
        grant.project_id = project_id.clone();
        grant.repo_commit_hash = repo_commit_hash;
        grant.current_stage = AuditStage::Initialized;
        grant.total_funded = GRANT_TOTAL_AMOUNT;
        grant.total_disbursed = 0;
        grant.bump = ctx.bumps.grant_account;
        grant.vault_bump = ctx.bumps.vault_token_account;
        grant.created_at = Clock::get()?.unix_timestamp;

        // Transfer 4,000 USDC from funder to Vault PDA
        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), GRANT_TOTAL_AMOUNT)?;

        emit!(GrantInitializedEvent {
            project_id,
            admin: grant.admin,
            auditor: grant.auditor,
            developer: grant.developer,
            total_grant: GRANT_TOTAL_AMOUNT,
            timestamp: grant.created_at,
        });

        Ok(())
    }

    /// Records audit findings and updates the audit stage. Callable only by designated Vaultline auditor.
    pub fn submit_audit_milestone(
        ctx: Context<SubmitAuditMilestone>,
        new_stage: AuditStage,
        audit_report_hash: [u8; 32],
        critical_findings: u16,
        high_findings: u16,
        medium_findings: u16,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(
            ctx.accounts.auditor.key() == grant.auditor,
            VaultlineError::UnauthorizedAuditor
        );

        // Enforce deterministic stage progression
        match new_stage {
            AuditStage::ScopingComplete => {
                require!(grant.current_stage == AuditStage::Initialized, VaultlineError::InvalidStageTransition);
            }
            AuditStage::FindingsDelivered => {
                require!(grant.current_stage == AuditStage::ScopingComplete, VaultlineError::InvalidStageTransition);
            }
            AuditStage::RemediationsVerified => {
                require!(grant.current_stage == AuditStage::FindingsDelivered, VaultlineError::InvalidStageTransition);
            }
            _ => return Err(VaultlineError::InvalidStageTransition.into()),
        }

        grant.current_stage = new_stage;
        grant.audit_report_hash = audit_report_hash;

        emit!(MilestoneSubmittedEvent {
            project_id: grant.project_id.clone(),
            stage: grant.current_stage,
            audit_report_hash,
            critical_findings,
            high_findings,
            medium_findings,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Releases escrowed milestone funds to the auditor/project developer upon milestone completion.
    pub fn release_milestone_funds(
        ctx: Context<ReleaseMilestoneFunds>,
        milestone: u8,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(
            ctx.accounts.signer.key() == grant.admin || ctx.accounts.signer.key() == grant.auditor,
            VaultlineError::UnauthorizedSigner
        );

        let amount_to_transfer: u64;

        if milestone == 1 {
            require!(grant.current_stage >= AuditStage::ScopingComplete, VaultlineError::MilestoneStageRequirementUnmet);
            require!(grant.total_disbursed == 0, VaultlineError::MilestoneAlreadyDisbursed);
            amount_to_transfer = MILESTONE_1_AMOUNT;
        } else if milestone == 2 {
            require!(grant.current_stage >= AuditStage::FindingsDelivered, VaultlineError::MilestoneStageRequirementUnmet);
            require!(grant.total_disbursed == MILESTONE_1_AMOUNT, VaultlineError::MilestoneAlreadyDisbursed);
            amount_to_transfer = MILESTONE_2_AMOUNT;
        } else if milestone == 3 {
            require!(grant.current_stage == AuditStage::RemediationsVerified, VaultlineError::MilestoneStageRequirementUnmet);
            require!(grant.total_disbursed == MILESTONE_1_AMOUNT + MILESTONE_2_AMOUNT, VaultlineError::MilestoneAlreadyDisbursed);
            amount_to_transfer = MILESTONE_3_AMOUNT;
        } else {
            return Err(VaultlineError::InvalidMilestoneNumber.into());
        }

        // PDA Seeds for signing
        let project_id_bytes = grant.project_id.as_bytes();
        let seeds: &[&[&[u8]]] = &[&[
            b"grant_vault",
            project_id_bytes,
            &[grant.vault_bump],
        ]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: ctx.accounts.vault_token_account.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, seeds),
            amount_to_transfer,
        )?;

        grant.total_disbursed = grant.total_disbursed.checked_add(amount_to_transfer).ok_or(VaultlineError::MathOverflow)?;

        emit!(FundsDisbursedEvent {
            project_id: grant.project_id.clone(),
            milestone,
            amount: amount_to_transfer,
            total_disbursed: grant.total_disbursed,
            recipient: ctx.accounts.auditor_token_account.key(),
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// ----------------------------------------------------------------------------
// ACCOUNTS CONTEXT
// ----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(project_id: String)]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + GrantEscrowAccount::LEN,
        seeds = [b"grant_state", project_id.as_bytes()],
        bump
    )]
    pub grant_account: Account<'info, GrantEscrowAccount>,

    #[account(
        init,
        payer = admin,
        seeds = [b"grant_vault", project_id.as_bytes()],
        bump,
        token::mint = usdc_mint,
        token::authority = vault_token_account,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub admin: Signer<'info>,

    /// CHECK: Validated as auditor in state
    pub auditor: AccountInfo<'info>,

    /// CHECK: Colosseum Developer beneficiary
    pub developer: AccountInfo<'info>,

    pub usdc_mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = admin
    )]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct SubmitAuditMilestone<'info> {
    #[account(
        mut,
        seeds = [b"grant_state", grant_account.project_id.as_bytes()],
        bump = grant_account.bump,
        has_one = auditor @ VaultlineError::UnauthorizedAuditor
    )]
    pub grant_account: Account<'info, GrantEscrowAccount>,

    pub auditor: Signer<'info>,
}

#[derive(Accounts)]
pub struct ReleaseMilestoneFunds<'info> {
    #[account(
        mut,
        seeds = [b"grant_state", grant_account.project_id.as_bytes()],
        bump = grant_account.bump
    )]
    pub grant_account: Account<'info, GrantEscrowAccount>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_account.project_id.as_bytes()],
        bump = grant_account.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = grant_account.usdc_mint
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

// ----------------------------------------------------------------------------
// STATE STRUCTURES & ENUMS
// ----------------------------------------------------------------------------

#[account]
pub struct GrantEscrowAccount {
    pub admin: Pubkey,                // 32
    pub auditor: Pubkey,              // 32
    pub developer: Pubkey,            // 32
    pub usdc_mint: Pubkey,            // 32
    pub vault_token_account: Pubkey,  // 32
    pub project_id: String,           // 4 + 64
    pub repo_commit_hash: [u8; 32],   // 32
    pub audit_report_hash: [u8; 32],  // 32
    pub current_stage: AuditStage,    // 1
    pub total_funded: u64,            // 8
    pub total_disbursed: u64,         // 8
    pub bump: u8,                     // 1
    pub vault_bump: u8,               // 1
    pub created_at: i64,              // 8
}

impl GrantEscrowAccount {
    pub const LEN: usize = 32 + 32 + 32 + 32 + 32 + (4 + 64) + 32 + 32 + 1 + 8 + 8 + 1 + 1 + 8;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum AuditStage {
    Initialized = 0,
    ScopingComplete = 1,
    FindingsDelivered = 2,
    RemediationsVerified = 3,
}

// ----------------------------------------------------------------------------
// EVENTS
// ----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub project_id: String,
    pub admin: Pubkey,
    pub auditor: Pubkey,
    pub developer: Pubkey,
    pub total_grant: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub project_id: String,
    pub stage: AuditStage,
    pub audit_report_hash: [u8; 32],
    pub critical_findings: u16,
    pub high_findings: u16,
    pub medium_findings: u16,
    pub timestamp: i64,
}

#[event]
pub struct FundsDisbursedEvent {
    pub project_id: String,
    pub milestone: u8,
    pub amount: u64,
    pub total_disbursed: u64,
    pub recipient: Pubkey,
    pub timestamp: i64,
}

// ----------------------------------------------------------------------------
// ERROR CODES
// ----------------------------------------------------------------------------

#[error_code]
pub enum VaultlineError {
    #[msg("Project ID must be between 1 and 64 characters.")]
    InvalidProjectIdLength,
    #[msg("Signer is not authorized to act as the auditor.")]
    UnauthorizedAuditor,
    #[msg("Signer is not authorized to trigger grant disbursement.")]
    UnauthorizedSigner,
    #[msg("Invalid audit stage transition.")]
    InvalidStageTransition,
    #[msg("Required milestone stage has not been reached.")]
    MilestoneStageRequirementUnmet,
    #[msg("Milestone has already been disbursed.")]
    MilestoneAlreadyDisbursed,
    #[msg("Invalid milestone index specified (Expected 1, 2, or 3).")]
    InvalidMilestoneNumber,
    #[msg("Arithmetic overflow detected.")]
    MathOverflow,
}