use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::state::*;

#[derive(Accounts)]
pub struct FundGrant<'info> {
    #[account(
        seeds = [b"config"],
        bump = config.bump
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
        token::mint = usdc_mint
    )]
    pub escrow_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        token::mint = usdc_mint,
        token::authority = funder
    )]
    pub funder_token_account: Account<'info, TokenAccount>,

    #[account(address = config.usdc_mint)]
    pub usdc_mint: Account<'info, Mint>,

    #[account(mut)]
    pub funder: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handler(ctx: Context<FundGrant>) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    require!(
        grant.status == GrantStatus::Initialized,
        VaultlineError::InvalidGrantStatus
    );

    let cpi_accounts = Transfer {
        from: ctx.accounts.funder_token_account.to_account_info(),
        to: ctx.accounts.escrow_vault.to_account_info(),
        authority: ctx.accounts.funder.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new(cpi_program, cpi_accounts);
    token::transfer(cpi_ctx, grant.total_amount)?;

    grant.status = GrantStatus::Funded;
    Ok(())
}