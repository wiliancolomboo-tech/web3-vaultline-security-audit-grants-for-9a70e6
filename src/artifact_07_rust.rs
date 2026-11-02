pub mod errors;
pub mod state;

use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::state::*;

declare_id!("VLinE11111111111111111111111111111111111111");

pub const GRANT_TOTAL_USDC: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const MILESTONE_ONE_USDC: u64 = 2_000_000_000; // 50% = 2,000 USDC
pub const MILESTONE_TWO_USDC: u64 = 2_000_000_000; // 50% = 2,000 USDC
pub const REFUND_TIMEOUT_SECONDS: i64 = 60 * 60 * 24 * 60; // 60 days

#[program]
pub mod vaultline_grants {
    use super::*;

    pub fn initialize_protocol(
        ctx: Context<InitializeProtocol>,
        vaultline_auditor: Pubkey,
    ) -> Result<()> {
        let config = &mut ctx.accounts.config;
        config.super_admin = ctx.accounts.admin.key();
        config.vaultline_auditor = vaultline_auditor;
        config.usdc_mint = ctx.accounts.usdc_mint.key();
        config.grant_amount = GRANT_TOTAL_USDC;
        config.total_grants_funded = 0;
        config.bump = ctx.bumps.config;
        Ok(())
    }

    pub fn apply_for_grant(
        ctx: Context<ApplyForGrant>,
        colosseum_project_id: [u8; 32],
        repo_commit_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let clock = Clock::get()?;

        grant.project_authority = ctx.accounts.project_authority.key();
        grant.colosseum_project_id = colosseum_project_id;
        grant.repo_commit_hash = repo_commit_hash;
        grant.audit_report_cid = [0u8; 32];
        grant.grant_amount = GRANT_TOTAL_USDC;
        grant.disbursed_amount = 0;
        grant.status = GrantStatus::Applied;
        grant.application_timestamp = clock.unix_timestamp;
        grant.approval_timestamp = 0;
        grant.completion_timestamp = 0;
        grant.bump = ctx.bumps.grant;
        grant.vault_bump = ctx.bumps.grant_vault;

        emit!(GrantAppliedEvent {
            project_authority: grant.project_authority,
            colosseum_project_id,
            timestamp: grant.application_timestamp,
        });

        Ok(())
    }

    pub fn approve_and_fund_grant(ctx: Context<ApproveAndFundGrant>) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let config = &mut ctx.accounts.config;
        let clock = Clock::get()?;

        require!(grant.status == GrantStatus::Applied, VaultlineError::InvalidGrantStatus);

        // Transfer 4,000 USDC from sponsor account into PDA grant vault
        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.grant_vault.to_account_info(),
            authority: ctx.accounts.admin.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(CpiContext::new(cpi_program, cpi_accounts), GRANT_TOTAL_USDC)?;

        grant.status = GrantStatus::ApprovedAndFunded;
        grant.approval_timestamp = clock.unix_timestamp;
        config.total_grants_funded = config.total_grants_funded.checked_add(1).ok_or(VaultlineError::MathOverflow)?;

        emit!(GrantApprovedEvent {
            colosseum_project_id: grant.colosseum_project_id,
            amount: GRANT_TOTAL_USDC,
            timestamp: grant.approval_timestamp,
        });

        Ok(())
    }

    pub fn attest_triage_complete(
        ctx: Context<AttestTriageComplete>,
        interim_findings_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        require!(grant.status == GrantStatus::ApprovedAndFunded, VaultlineError::InvalidGrantStatus);

        let colosseum_id = grant.colosseum_project_id;
        let seeds = &[
            b"grant_vault".as_ref(),
            colosseum_id.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // Release Milestone 1: 2,000 USDC (50%)
        let cpi_accounts = Transfer {
            from: ctx.accounts.grant_vault.to_account_info(),
            to: ctx.accounts.project_token_account.to_account_info(),
            authority: ctx.accounts.grant_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            MILESTONE_ONE_USDC,
        )?;

        grant.disbursed_amount = grant.disbursed_amount
            .checked_add(MILESTONE_ONE_USDC)
            .ok_or(VaultlineError::MathOverflow)?;
        grant.status = GrantStatus::TriageAttested;

        emit!(MilestoneDisbursedEvent {
            colosseum_project_id: grant.colosseum_project_id,
            milestone_id: 1,
            amount: MILESTONE_ONE_USDC,
            evidence_hash: interim_findings_hash,
        });

        Ok(())
    }

    pub fn attest_final_remediation(
        ctx: Context<AttestFinalRemediation>,
        final_report_cid: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let clock = Clock::get()?;
        require!(grant.status == GrantStatus::TriageAttested, VaultlineError::InvalidGrantStatus);

        let colosseum_id = grant.colosseum_project_id;
        let seeds = &[
            b"grant_vault".as_ref(),
            colosseum_id.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        // Release Milestone 2: Remaining 2,000 USDC (50%)
        let remaining_balance = ctx.accounts.grant_vault.amount;
        require!(remaining_balance >= MILESTONE_TWO_USDC, VaultlineError::InsufficientVaultBalance);

        let cpi_accounts = Transfer {
            from: ctx.accounts.grant_vault.to_account_info(),
            to: ctx.accounts.project_token_account.to_account_info(),
            authority: ctx.accounts.grant_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            MILESTONE_TWO_USDC,
        )?;

        grant.disbursed_amount = grant.disbursed_amount
            .checked_add(MILESTONE_TWO_USDC)
            .ok_or(VaultlineError::MathOverflow)?;
        grant.audit_report_cid = final_report_cid;
        grant.status = GrantStatus::Completed;
        grant.completion_timestamp = clock.unix_timestamp;

        emit!(GrantCompletedEvent {
            colosseum_project_id: grant.colosseum_project_id,
            final_report_cid,
            timestamp: grant.completion_timestamp,
        });

        Ok(())
    }

    pub fn emergency_refund(ctx: Context<EmergencyRefund>) -> Result<()> {
        let grant = &mut ctx.accounts.grant;
        let clock = Clock::get()?;

        // Grant can only be refunded if it has been abandoned past REFUND_TIMEOUT_SECONDS
        require!(
            grant.status == GrantStatus::ApprovedAndFunded || grant.status == GrantStatus::TriageAttested,
            VaultlineError::InvalidGrantStatus
        );

        let elapsed = clock.unix_timestamp
            .checked_sub(grant.approval_timestamp)
            .ok_or(VaultlineError::MathOverflow)?;

        require!(elapsed >= REFUND_TIMEOUT_SECONDS, VaultlineError::GrantTimeoutNotReached);

        let colosseum_id = grant.colosseum_project_id;
        let seeds = &[
            b"grant_vault".as_ref(),
            colosseum_id.as_ref(),
            &[grant.vault_bump],
        ];
        let signer_seeds = &[&seeds[..]];

        let refund_balance = ctx.accounts.grant_vault.amount;
        let cpi_accounts = Transfer {
            from: ctx.accounts.grant_vault.to_account_info(),
            to: ctx.accounts.admin_token_account.to_account_info(),
            authority: ctx.accounts.grant_vault.to_account_info(),
        };
        let cpi_program = ctx.accounts.token_program.to_account_info();
        token::transfer(
            CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds),
            refund_balance,
        )?;

        grant.status = GrantStatus::Cancelled;

        emit!(GrantRefundedEvent {
            colosseum_project_id: grant.colosseum_project_id,
            refund_amount: refund_balance,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT VALIDATION CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializeProtocol<'info> {
    #[account(
        init,
        payer = admin,
        space = ProtocolConfig::LEN,
        seeds = [b"vaultline_config"],
        bump
    )]
    pub config: Account<'info, ProtocolConfig>,
    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(colosseum_project_id: [u8; 32])]
pub struct ApplyForGrant<'info> {
    #[account(
        init,
        payer = project_authority,
        space = GrantApplication::LEN,
        seeds = [b"grant_application", colosseum_project_id.as_ref()],
        bump
    )]
    pub grant: Account<'info, GrantApplication>,

    #[account(
        init,
        payer = project_authority,
        seeds = [b"grant_vault", colosseum_project_id.as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = grant_vault
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,
    #[account(mut)]
    pub project_authority: Signer<'info>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct ApproveAndFundGrant<'info> {
    #[account(
        mut,
        seeds = [b"vaultline_config"],
        bump = config.bump,
        has_one = super_admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [b"grant_application", grant.colosseum_project_id.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantApplication>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant.colosseum_project_id.as_ref()],
        bump = grant.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = funder_token_account.mint == config.usdc_mint,
        constraint = funder_token_account.owner == admin.key()
    )]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub super_admin: Signer<'info>,
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AttestTriageComplete<'info> {
    #[account(
        seeds = [b"vaultline_config"],
        bump = config.bump,
        has_one = vaultline_auditor @ VaultlineError::UnauthorizedAuditor
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [b"grant_application", grant.colosseum_project_id.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantApplication>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant.colosseum_project_id.as_ref()],
        bump = grant.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = project_token_account.owner == grant.project_authority,
        constraint = project_token_account.mint == config.usdc_mint
    )]
    pub project_token_account: Account<'info, TokenAccount>,

    pub vaultline_auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct AttestFinalRemediation<'info> {
    #[account(
        seeds = [b"vaultline_config"],
        bump = config.bump,
        has_one = vaultline_auditor @ VaultlineError::UnauthorizedAuditor
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [b"grant_application", grant.colosseum_project_id.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantApplication>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant.colosseum_project_id.as_ref()],
        bump = grant.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = project_token_account.owner == grant.project_authority,
        constraint = project_token_account.mint == config.usdc_mint
    )]
    pub project_token_account: Account<'info, TokenAccount>,

    pub vaultline_auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct EmergencyRefund<'info> {
    #[account(
        seeds = [b"vaultline_config"],
        bump = config.bump,
        has_one = super_admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub config: Account<'info, ProtocolConfig>,

    #[account(
        mut,
        seeds = [b"grant_application", grant.colosseum_project_id.as_ref()],
        bump = grant.bump
    )]
    pub grant: Account<'info, GrantApplication>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant.colosseum_project_id.as_ref()],
        bump = grant.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.owner == super_admin.key(),
        constraint = admin_token_account.mint == config.usdc_mint
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    pub super_admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

// -----------------------------------------------------------------------------
// EVENT DEFINITIONS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantAppliedEvent {
    pub project_authority: Pubkey,
    pub colosseum_project_id: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct GrantApprovedEvent {
    pub colosseum_project_id: [u8; 32],
    pub amount: u64,
    pub timestamp: i64,
}

#[event]
pub struct MilestoneDisbursedEvent {
    pub colosseum_project_id: [u8; 32],
    pub milestone_id: u8,
    pub amount: u64,
    pub evidence_hash: [u8; 32],
}

#[event]
pub struct GrantCompletedEvent {
    pub colosseum_project_id: [u8; 32],
    pub final_report_cid: [u8; 32],
    pub timestamp: i64,
}

#[event]
pub struct GrantRefundedEvent {
    pub colosseum_project_id: [u8; 32],
    pub refund_amount: u64,
}