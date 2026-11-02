use anchor_lang::prelude::*;
use anchor_spl::token::{self, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::events::EmergencyRefundEvent;
use crate::state::{AuditGrant, GrantStatus, ProtocolConfig, ESCROW_VAULT_SEED, GRANT_SEED, PROTOCOL_CONFIG_SEED};

#[derive(Accounts)]
pub struct EmergencyRefund<'info> {
    #[account(
        seeds = [PROTOCOL_CONFIG_SEED],
        bump = config.bump,
        has_one = admin @ VaultlineError::UnauthorizedAdmin
    )]
    pub config: Account<'info, ProtocolConfig>,
    #[account(
        mut,
        seeds = [GRANT_SEED, grant.project_slug.as_bytes()],
        bump = grant.grant_bump,
        constraint = grant.status != GrantStatus::Completed && grant.status != GrantStatus::Refunded @ VaultlineError::InvalidStatusTransition
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
        constraint = admin_token_account.owner == config.admin
    )]
    pub admin_token_account: Account<'info, TokenAccount>,
    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<EmergencyRefund>) -> Result<()> {
    let clock = Clock::get()?;
    let grant = &mut ctx.accounts.grant;
    let config = &ctx.accounts.config;

    // SLA timeout check: grant funded > sla_timeout_seconds without completion
    let elapsed = clock.unix_timestamp.checked_sub(grant.funded_at).ok_or(VaultlineError::MathOverflow)?;
    require!(
        elapsed >= config.sla_timeout_seconds,
        VaultlineError::SLANotExpired
    );

    let remaining_balance = ctx.accounts.escrow_vault.amount;
    let project_slug_bytes = grant.project_slug.as_bytes();
    let grant_seeds: &[&[u8]] = &[
        GRANT_SEED,
        project_slug_bytes,
        &[grant.grant_bump],
    ];
    let signer = &[grant_seeds];

    let cpi_accounts = Transfer {
        from: ctx.accounts.escrow_vault.to_account_info(),
        to: ctx.accounts.admin_token_account.to_account_info(),
        authority: grant.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer,
    );
    token::transfer(cpi_ctx, remaining_balance)?;

    grant.status = GrantStatus::Refunded;

    emit!(EmergencyRefundEvent {
        grant_pubkey: grant.key(),
        refunded_amount: remaining_balance,
        recipient: ctx.accounts.admin.key(),
        timestamp: clock.unix_timestamp,
    });

    Ok(())
}