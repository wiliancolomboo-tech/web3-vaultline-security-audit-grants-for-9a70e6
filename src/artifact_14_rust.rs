use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::events::MilestoneDisbursedEvent;
use crate::state::{AuditGrant, GrantStatus, ProtocolConfig, ESCROW_VAULT_SEED, GRANT_SEED, PROTOCOL_CONFIG_SEED};

#[derive(Accounts)]
pub struct DisburseMilestone<'info> {
    #[account(
        mut,
        seeds = [PROTOCOL_CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub config: Account<'info, ProtocolConfig>,
    #[account(
        mut,
        seeds = [GRANT_SEED, grant.project_slug.as_bytes()],
        bump = grant.grant_bump,
        has_one = applicant_authority @ VaultlineError::UnauthorizedApplicant
    )]
    pub grant: Account<'info, AuditGrant>,
    #[account(
        mut,
        seeds = [ESCROW_VAULT_SEED, grant.key().as_ref()],
        bump = grant.vault_bump
    )]
    pub escrow_vault: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = auditor_token_account.owner == grant.auditor_pubkey
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,
    #[account(
        mut,
        constraint = project_token_account.owner == grant.applicant_authority
    )]
    pub project_token_account: Account<'info, TokenAccount>,
    pub admin: Signer<'info>,
    /// CHECK: Validated against grant.applicant_authority
    pub applicant_authority: AccountInfo<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(
    ctx: Context<DisburseMilestone>,
    milestone_index: u8,
    auditor_bps: u16,
) -> Result<()> {
    require!(auditor_bps <= 10_000, VaultlineError::InvalidBasisPoints);
    let grant = &mut ctx.accounts.grant;

    let tranche_amount = match (milestone_index, grant.status) {
        (1, GrantStatus::ReportSubmitted) => {
            grant.status = GrantStatus::MilestoneOneDisbursed;
            grant.funded_amount / 2
        }
        (2, GrantStatus::MilestoneOneDisbursed) => {
            grant.status = GrantStatus::Completed;
            grant.funded_amount.checked_sub(grant.disbursed_amount).ok_or(VaultlineError::MathOverflow)?
        }
        _ => return Err(VaultlineError::InvalidStatusTransition.into()),
    };

    let auditor_payout = (tranche_amount as u128)
        .checked_mul(auditor_bps as u128)
        .ok_or(VaultlineError::MathOverflow)?
        .checked_div(10_000)
        .ok_or(VaultlineError::MathOverflow)? as u64;
    let project_payout = tranche_amount.checked_sub(auditor_payout).ok_or(VaultlineError::MathOverflow)?;

    let project_slug_bytes = grant.project_slug.as_bytes();
    let grant_seeds: &[&[u8]] = &[
        GRANT_SEED,
        project_slug_bytes,
        &[grant.grant_bump],
    ];
    let signer = &[grant_seeds];

    // Transfer to Auditor
    if auditor_payout > 0 {
        let auditor_cpi = Transfer {
            from: ctx.accounts.escrow_vault.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let auditor_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            auditor_cpi,
            signer,
        );
        token::transfer(auditor_ctx, auditor_payout)?;
    }

    // Transfer remainder to Project
    if project_payout > 0 {
        let project_cpi = Transfer {
            from: ctx.accounts.escrow_vault.to_account_info(),
            to: ctx.accounts.project_token_account.to_account_info(),
            authority: grant.to_account_info(),
        };
        let project_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            project_cpi,
            signer,
        );
        token::transfer(project_ctx, project_payout)?;
    }

    grant.disbursed_amount = grant
        .disbursed_amount
        .checked_add(tranche_amount)
        .ok_or(VaultlineError::MathOverflow)?;

    let config = &mut ctx.accounts.config;
    config.total_usdc_disbursed = config
        .total_usdc_disbursed
        .checked_add(tranche_amount)
        .ok_or(VaultlineError::MathOverflow)?;

    emit!(MilestoneDisbursedEvent {
        grant_pubkey: grant.key(),
        milestone_index,
        auditor_payout,
        project_payout,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}