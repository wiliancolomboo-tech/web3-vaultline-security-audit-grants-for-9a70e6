use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("VLinEK3yZ5Xg8V9Pj1R7u1z5iY7L8sZ8xQGv9aK8oJ2");

pub const TOTAL_GRANT_AMOUNT: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const TRANCHE_1_AMOUNT: u64 = 1_000_000_000;   // 1,000 USDC
pub const TRANCHE_2_AMOUNT: u64 = 1_500_000_000;   // 1,500 USDC
pub const TRANCHE_3_AMOUNT: u64 = 1_500_000_000;   // 1,500 USDC

#[program]
pub mod vaultline_grant_core {
    use super::*;

    /// Initialize a new 4,000 USDC Security Audit Grant for a Colosseum Project
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        colosseum_project_id: String,
        repo_commit_hash: [u8; 32],
    ) -> Result<()> {
        require!(
            colosseum_project_id.len() > 0 && colosseum_project_id.len() <= 64,
            GrantError::InvalidProjectId
        );

        let grant = &mut ctx.accounts.grant_state;
        grant.admin = ctx.accounts.admin.key();
        grant.auditor = ctx.accounts.auditor.key();
        grant.project_lead = ctx.accounts.project_lead.key();
        grant.usdc_mint = ctx.accounts.usdc_mint.key();
        grant.colosseum_project_id = colosseum_project_id.clone();
        grant.repo_commit_hash = repo_commit_hash;
        grant.current_milestone = Milestone::Initialized;
        grant.total_funded = 0;
        grant.total_disbursed = 0;
        grant.bump = ctx.bumps.grant_state;
        grant.vault_bump = ctx.bumps.vault_token_account;

        emit!(GrantInitializedEvent {
            grant: grant.key(),
            admin: grant.admin,
            project_id: colosseum_project_id,
            auditor: grant.auditor,
            project_lead: grant.project_lead,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Fund the escrow account with up to 4,000 USDC
    pub fn fund_escrow(ctx: Context<FundEscrow>, amount: u64) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require!(
            grant.total_funded.checked_add(amount).unwrap() <= TOTAL_GRANT_AMOUNT,
            GrantError::ExceedsMaxGrantAmount
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.funder.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        grant.total_funded = grant.total_funded.checked_add(amount).unwrap();

        emit!(EscrowFundedEvent {
            grant: grant.key(),
            funder: ctx.accounts.funder.key(),
            amount,
            total_funded: grant.total_funded,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Auditor submits milestone proof (SHA-256 of report artifact)
    pub fn submit_milestone_artifact(
        ctx: Context<SubmitMilestoneArtifact>,
        milestone: Milestone,
        artifact_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require_keys_eq!(ctx.accounts.auditor.key(), grant.auditor, GrantError::UnauthorizedAuditor);

        match milestone {
            Milestone::ScopeApproved => {
                require!(grant.current_milestone == Milestone::Initialized, GrantError::InvalidStateSequence);
                grant.tranche_1_hash = artifact_hash;
            }
            Milestone::FindingsTriaged => {
                require!(grant.current_milestone == Milestone::ScopeApproved, GrantError::InvalidStateSequence);
                grant.tranche_2_hash = artifact_hash;
            }
            Milestone::RemediationsSignedOff => {
                require!(grant.current_milestone == Milestone::FindingsTriaged, GrantError::InvalidStateSequence);
                grant.tranche_3_hash = artifact_hash;
            }
            _ => return err!(GrantError::InvalidMilestoneSubmission),
        }

        emit!(ArtifactSubmittedEvent {
            grant: grant.key(),
            milestone,
            artifact_hash,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Vaultline Admin approves milestone and releases the tranche payment to Auditor
    pub fn approve_and_disburse_tranche(
        ctx: Context<ApproveAndDisburseTranche>,
        milestone: Milestone,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require_keys_eq!(ctx.accounts.admin.key(), grant.admin, GrantError::UnauthorizedAdmin);

        let payout_amount: u64 = match milestone {
            Milestone::ScopeApproved => {
                require!(grant.current_milestone == Milestone::Initialized, GrantError::InvalidStateSequence);
                require!(grant.tranche_1_hash != [0u8; 32], GrantError::MissingArtifactHash);
                grant.current_milestone = Milestone::ScopeApproved;
                TRANCHE_1_AMOUNT
            }
            Milestone::FindingsTriaged => {
                require!(grant.current_milestone == Milestone::ScopeApproved, GrantError::InvalidStateSequence);
                require!(grant.tranche_2_hash != [0u8; 32], GrantError::MissingArtifactHash);
                grant.current_milestone = Milestone::FindingsTriaged;
                TRANCHE_2_AMOUNT
            }
            Milestone::RemediationsSignedOff => {
                require!(grant.current_milestone == Milestone::FindingsTriaged, GrantError::InvalidStateSequence);
                require!(grant.tranche_3_hash != [0u8; 32], GrantError::MissingArtifactHash);
                grant.current_milestone = Milestone::RemediationsSignedOff;
                TRANCHE_3_AMOUNT
            }
            _ => return err!(GrantError::InvalidMilestoneApproval),
        };

        require!(
            ctx.accounts.vault_token_account.amount >= payout_amount,
            GrantError::InsufficientEscrowBalance
        );

        // Sign CPI transfer with PDA seeds
        let admin_key = grant.admin;
        let project_id_bytes = grant.colosseum_project_id.as_bytes();
        let bump_seeds = &[
            b"vaultline_grant",
            admin_key.as_ref(),
            project_id_bytes,
            &[grant.bump],
        ];
        let signer_seeds = &[&bump_seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.vault_token_account.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
        token::transfer(cpi_ctx, payout_amount)?;

        grant.total_disbursed = grant.total_disbursed.checked_add(payout_amount).unwrap();

        emit!(MilestoneDisbursedEvent {
            grant: grant.key(),
            milestone,
            recipient: ctx.accounts.auditor_token_account.key(),
            amount: payout_amount,
            total_disbursed: grant.total_disbursed,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }

    /// Emergency cancellation & refund (available only before any tranche is disbursed)
    pub fn cancel_and_refund(ctx: Context<CancelAndRefund>) -> Result<()> {
        let grant = &mut ctx.accounts.grant_state;
        require_keys_eq!(ctx.accounts.admin.key(), grant.admin, GrantError::UnauthorizedAdmin);
        require!(grant.total_disbursed == 0, GrantError::CannotCancelDisbursedGrant);

        let refund_balance = ctx.accounts.vault_token_account.amount;
        if refund_balance > 0 {
            let admin_key = grant.admin;
            let project_id_bytes = grant.colosseum_project_id.as_bytes();
            let bump_seeds = &[
                b"vaultline_grant",
                admin_key.as_ref(),
                project_id_bytes,
                &[grant.bump],
            ];
            let signer_seeds = &[&bump_seeds[..]];

            let cpi_accounts = Transfer {
                from: ctx.accounts.vault_token_account.to_account_info(),
                to: ctx.accounts.admin_token_account.to_account_info(),
                authority: grant.to_account_info(),
            };
            let cpi_program = ctx.accounts.token_program.to_account_info();
            let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
            token::transfer(cpi_ctx, refund_balance)?;
        }

        grant.current_milestone = Milestone::Cancelled;

        emit!(GrantCancelledEvent {
            grant: grant.key(),
            refunded_amount: refund_balance,
            timestamp: Clock::get()?.unix_timestamp,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(colosseum_project_id: String, repo_commit_hash: [u8; 32])]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = admin,
        space = GrantState::LEN,
        seeds = [b"vaultline_grant", admin.key().as_ref(), colosseum_project_id.as_bytes()],
        bump
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        init,
        payer = admin,
        seeds = [b"grant_vault", grant_state.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = grant_state
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    /// CHECK: Auditor verified by Vaultline
    pub auditor: UncheckedAccount<'info>,

    /// CHECK: Colosseum Project Founder
    pub project_lead: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct FundEscrow<'info> {
    #[account(
        mut,
        seeds = [b"vaultline_grant", grant_state.admin.as_ref(), grant_state.colosseum_project_id.as_bytes()],
        bump = grant_state.bump
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_state.key().as_ref()],
        bump = grant_state.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub funder: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct SubmitMilestoneArtifact<'info> {
    #[account(
        mut,
        seeds = [b"vaultline_grant", grant_state.admin.as_ref(), grant_state.colosseum_project_id.as_bytes()],
        bump = grant_state.bump
    )]
    pub grant_state: Account<'info, GrantState>,

    pub auditor: Signer<'info>,
}

#[derive(Accounts)]
pub struct ApproveAndDisburseTranche<'info> {
    #[account(
        mut,
        seeds = [b"vaultline_grant", grant_state.admin.as_ref(), grant_state.colosseum_project_id.as_bytes()],
        bump = grant_state.bump
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_state.key().as_ref()],
        bump = grant_state.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.owner == grant_state.auditor @ GrantError::AuditorTokenAccountMismatch,
        constraint = auditor_token_account.mint == grant_state.usdc_mint @ GrantError::MintMismatch
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct CancelAndRefund<'info> {
    #[account(
        mut,
        seeds = [b"vaultline_grant", grant_state.admin.as_ref(), grant_state.colosseum_project_id.as_bytes()],
        bump = grant_state.bump
    )]
    pub grant_state: Account<'info, GrantState>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_state.key().as_ref()],
        bump = grant_state.vault_bump
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.owner == grant_state.admin @ GrantError::AdminTokenAccountMismatch,
        constraint = admin_token_account.mint == grant_state.usdc_mint @ GrantError::MintMismatch
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// STATE STRUCTURES & ENUMS
// -----------------------------------------------------------------------------

#[account]
pub struct GrantState {
    pub admin: Pubkey,                 // 32
    pub auditor: Pubkey,               // 32
    pub project_lead: Pubkey,          // 32
    pub usdc_mint: Pubkey,             // 32
    pub colosseum_project_id: String,  // 4 + 64
    pub repo_commit_hash: [u8; 32],    // 32
    pub current_milestone: Milestone,  // 1
    pub tranche_1_hash: [u8; 32],      // 32
    pub tranche_2_hash: [u8; 32],      // 32
    pub tranche_3_hash: [u8; 32],      // 32
    pub total_funded: u64,             // 8
    pub total_disbursed: u64,          // 8
    pub bump: u8,                      // 1
    pub vault_bump: u8,                // 1
}

impl GrantState {
    pub const LEN: usize = 8   // discriminator
        + 32                   // admin
        + 32                   // auditor
        + 32                   // project_lead
        + 32                   // usdc_mint
        + (4 + 64)             // colosseum_project_id string
        + 32                   // repo_commit_hash
        + 1                    // current_milestone
        + 32                   // tranche_1_hash
        + 32                   // tranche_2_hash
        + 32                   // tranche_3_hash
        + 8                    // total_funded
        + 8                    // total_disbursed
        + 1                    // bump
        + 1                    // vault_bump
        + 64;                  // storage padding
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum Milestone {
    Initialized,
    ScopeApproved,
    FindingsTriaged,
    RemediationsSignedOff,
    Cancelled,
}

// -----------------------------------------------------------------------------
// EVENTS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantInitializedEvent {
    pub grant: Pubkey,
    pub admin: Pubkey,
    pub project_id: String,
    pub auditor: Pubkey,
    pub project_lead: Pubkey,
    pub timestamp: i64,
}

#[event]
pub struct EscrowFundedEvent {
    pub grant: Pubkey,
    pub funder: Pubkey,
    pub amount: u64,
    pub total_funded: u64,
    pub timestamp: i64,
}

#[event]
pub struct ArtifactSubmittedEvent {
    pub grant: Pubkey,
    pub milestone: Milestone,
    pub artifact_hash: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct MilestoneDisbursedEvent {
    pub grant: Pubkey,
    pub milestone: Milestone,
    pub recipient: Pubkey,
    pub amount: u64,
    pub total_disbursed: u64,
    pub timestamp: i64,
}

#[event]
pub struct GrantCancelledEvent {
    pub grant: Pubkey,
    pub refunded_amount: u64,
    pub timestamp: i64,
}

// -----------------------------------------------------------------------------
// ERRORS
// -----------------------------------------------------------------------------

#[error_code]
pub enum GrantError {
    #[msg("Project ID must be between 1 and 64 characters.")]
    InvalidProjectId,
    #[msg("Funding amount exceeds the maximum 4,000 USDC grant allotment.")]
    ExceedsMaxGrantAmount,
    #[msg("Signer is not the authorized auditor.")]
    UnauthorizedAuditor,
    #[msg("Signer is not the authorized Vaultline administrator.")]
    UnauthorizedAdmin,
    #[msg("Invalid sequential progression of milestone status.")]
    InvalidStateSequence,
    #[msg("Invalid milestone submission.")]
    InvalidMilestoneSubmission,
    #[msg("Invalid milestone approval.")]
    InvalidMilestoneApproval,
    #[msg("Cannot disburse milestone without a valid artifact hash.")]
    MissingArtifactHash,
    #[msg("Insufficient escrow vault balance for tranche payout.")]
    InsufficientEscrowBalance,
    #[msg("Auditor token account does not match registered auditor authority.")]
    AuditorTokenAccountMismatch,
    #[msg("Admin token account does not match registered admin authority.")]
    AdminTokenAccountMismatch,
    #[msg("Token account mint does not match the configured USDC mint.")]
    MintMismatch,
    #[msg("Cannot cancel grant escrow after tranches have been disbursed.")]
    CannotCancelDisbursedGrant,
}