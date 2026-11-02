use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::state::*;

#[derive(Accounts)]
pub struct EmergencyClawback<'info> {
    #[account(
        seeds = [b"config"],
        bump = config.bump,
        has_one = admin
    )]
    pub config: Account<'info, ProgramConfig>,

    #[account(
        mut,
        seeds = [b"grant", grant.grant_id.as_ref()],
        bump = grant.bump,
        has_one = escrow_vault
    )]
    pub grant: Account<'info, AuditGrant>,

    #[account(
        mut,
        seeds = [b"escrow", grant.key().as_ref()],
        bump = grant.vault_bump,
        token::mint = usdc_mint
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = admin
    )]
    pub admin_token_account: Account<'info, TokenAccount>,

    #[account(address = config.usdc_mint)]
    pub usdc_mint: Account<'info, Mint>,

    pub admin: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<EmergencyClawback>) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    let clock = Clock::get()?;

    require!(
        clock.unix_timestamp > grant.sla_deadline_ts,
        VaultlineError::SlaNotExpired
    );
    require!(
        grant.status != GrantStatus::Completed && grant.status != GrantStatus::Refunded,
        VaultlineError::InvalidGrantStatus
    );

    let remaining_balance = ctx.accounts.escrow_vault.amount;
    require!(remaining_balance > 0, VaultlineError::InsufficientVaultBalance);

    let grant_key = grant.key();
    let seeds = &[
        b"escrow",
        grant_key.as_ref(),
        &[grant.vault_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.escrow_vault.to_account_info(),
        to: ctx.accounts.admin_token_account.to_account_info(),
        authority: ctx.accounts.escrow_vault.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, remaining_balance)?;

    grant.status = GrantStatus::Refunded;
    Ok(())
}