use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::errors::VaultlineError;
use crate::state::*;

#[derive(Accounts)]
pub struct ReleaseMilestone<'info> {
    #[account(
        mut,
        seeds = [b"config"],
        bump = config.bump
    )]
    pub config: Account<'info, ProgramConfig>,

    #[account(
        mut,
        seeds = [b"grant", grant.grant_id.as_ref()],
        bump = grant.bump,
        has_one = escrow_vault,
        has_one = auditor
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
        token::authority = auditor
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    #[account(address = config.usdc_mint)]
    pub usdc_mint: Account<'info, Mint>,

    /// CHECK: Auditor or Grantee or Admin depending on milestone
    pub signer: Signer<'info>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_milestone_1(ctx: Context<ReleaseMilestone>) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    require!(
        grant.status == GrantStatus::InReview,
        VaultlineError::InvalidGrantStatus
    );
    require!(!grant.milestone_1_paid, VaultlineError::MilestoneAlreadyReleased);

    // Signer must be grantee confirming initial report receipt OR admin
    require!(
        ctx.accounts.signer.key() == grant.grantee_authority || 
        ctx.accounts.signer.key() == ctx.accounts.config.admin,
        VaultlineError::UnauthorizedSigner
    );

    let grant_key = grant.key();
    let seeds = &[
        b"escrow",
        grant_key.as_ref(),
        &[grant.vault_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.escrow_vault.to_account_info(),
        to: ctx.accounts.auditor_token_account.to_account_info(),
        authority: ctx.accounts.escrow_vault.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, grant.milestone_1_amount)?;

    grant.milestone_1_paid = true;
    grant.status = GrantStatus::MilestoneOneDisbursed;

    let config = &mut ctx.accounts.config;
    config.total_funds_disbursed = config.total_funds_disbursed
        .checked_add(grant.milestone_1_amount)
        .ok_or(VaultlineError::NumericalOverflow)?;

    Ok(())
}

pub fn handle_milestone_2(
    ctx: Context<ReleaseMilestone>,
    remediation_digest: [u8; 32],
) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    require!(
        grant.status == GrantStatus::MilestoneOneDisbursed,
        VaultlineError::InvalidGrantStatus
    );
    require!(!grant.milestone_2_paid, VaultlineError::MilestoneAlreadyReleased);
    require!(remediation_digest != [0u8; 32], VaultlineError::InvalidReportDigest);

    // Auditor must co-sign that remediation is completed
    require!(
        ctx.accounts.signer.key() == grant.auditor,
        VaultlineError::UnauthorizedSigner
    );

    let grant_key = grant.key();
    let seeds = &[
        b"escrow",
        grant_key.as_ref(),
        &[grant.vault_bump],
    ];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.escrow_vault.to_account_info(),
        to: ctx.accounts.auditor_token_account.to_account_info(),
        authority: ctx.accounts.escrow_vault.to_account_info(),
    };
    let cpi_program = ctx.accounts.token_program.to_account_info();
    let cpi_ctx = CpiContext::new_with_signer(cpi_program, cpi_accounts, signer_seeds);
    token::transfer(cpi_ctx, grant.milestone_2_amount)?;

    grant.remediation_digest = remediation_digest;
    grant.milestone_2_paid = true;
    grant.status = GrantStatus::Completed;

    let config = &mut ctx.accounts.config;
    config.total_funds_disbursed = config.total_funds_disbursed
        .checked_add(grant.milestone_2_amount)
        .ok_or(VaultlineError::NumericalOverflow)?;

    Ok(())
}