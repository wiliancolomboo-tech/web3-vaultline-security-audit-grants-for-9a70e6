use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};

declare_id!("V1tLineAuditGrant1111111111111111111111111111");

#[program]
pub mod vaultline_grant {
    use super::*;

    /// Initialize a new Vaultline Audit Grant for a Colosseum hackathon project.
    pub fn initialize_grant(
        ctx: Context<InitializeGrant>,
        project_id: String,
        grant_amount: u64,
        copay_amount: u64,
    ) -> Result<()> {
        require!(project_id.len() <= 64, GrantError::StringTooLong);
        require!(grant_amount > 0, GrantError::InvalidAmount);

        let grant = &mut ctx.accounts.grant_account;
        grant.admin = ctx.accounts.admin.key();
        grant.project_authority = ctx.accounts.project_authority.key();
        grant.auditor = ctx.accounts.auditor.key();
        grant.usdc_mint = ctx.accounts.usdc_mint.key();
        grant.grant_amount = grant_amount;
        grant.copay_amount = copay_amount;
        grant.total_funded = 0;
        grant.project_id = project_id;
        grant.scope_commit_hash = [0u8; 32];
        grant.state = GrantState::Created;
        grant.bump = ctx.bumps.grant_account;
        grant.vault_bump = ctx.bumps.grant_vault;

        emit!(GrantInitialized {
            grant: grant.key(),
            project: grant.project_authority,
            grant_amount,
            copay_amount,
        });

        Ok(())
    }

    /// Fund the grant with USDC (Vaultline grant portion or Project co-pay portion).
    pub fn fund_grant(ctx: Context<FundGrant>, amount: u64) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(
            grant.state == GrantState::Created || grant.state == GrantState::PartiallyFunded,
            GrantError::InvalidStateTransition
        );

        let cpi_accounts = Transfer {
            from: ctx.accounts.funder_token_account.to_account_info(),
            to: ctx.accounts.grant_vault.to_account_info(),
            authority: ctx.accounts.funder.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, amount)?;

        grant.total_funded = grant.total_funded.checked_add(amount).ok_or(GrantError::MathOverflow)?;
        let required_total = grant.grant_amount.checked_add(grant.copay_amount).ok_or(GrantError::MathOverflow)?;

        if grant.total_funded >= required_total {
            grant.state = GrantState::FullyFunded;
        } else {
            grant.state = GrantState::PartiallyFunded;
        }

        emit!(GrantFunded {
            grant: grant.key(),
            funder: ctx.accounts.funder.key(),
            amount,
            total_funded: grant.total_funded,
        });

        Ok(())
    }

    /// Record the cryptographic Git commit hash and AST scope digest of the Colosseum project.
    pub fn record_scope(
        ctx: Context<RecordScope>,
        commit_hash: [u8; 32],
        program_ids: Vec<Pubkey>,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(grant.state == GrantState::FullyFunded, GrantError::InvalidStateTransition);
        require!(program_ids.len() > 0 && program_ids.len() <= 8, GrantError::InvalidScopeList);

        grant.scope_commit_hash = commit_hash;
        grant.state = GrantState::ScopeLocked;

        // Tranche 1: Release 25% of grant upon formal scope verification
        let tranche_1 = grant.total_funded.checked_mul(25).unwrap().checked_div(100).unwrap();
        
        let seeds = &[
            b"grant_vault",
            grant.to_account_info().key.as_ref(),
            &[grant.vault_bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.grant_vault.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: ctx.accounts.grant_vault.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, tranche_1)?;

        emit!(ScopeLocked {
            grant: grant.key(),
            commit_hash,
            tranche_released: tranche_1,
        });

        Ok(())
    }

    /// Audit is completed: publish the immutable Attestation Certificate PDA and disburse final funds.
    pub fn finalize_audit_and_attest(
        ctx: Context<FinalizeAuditAndAttest>,
        report_hash: [u8; 32],
        critical_count: u8,
        high_count: u8,
        medium_count: u8,
        low_count: u8,
        security_score: u8,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.grant_account;
        require!(grant.state == GrantState::ScopeLocked, GrantError::InvalidStateTransition);
        require!(security_score <= 100, GrantError::InvalidScore);

        let attestation = &mut ctx.accounts.attestation_certificate;
        attestation.grant = grant.key();
        attestation.project_authority = grant.project_authority;
        attestation.auditor = grant.auditor;
        attestation.report_hash = report_hash;
        attestation.critical_issues = critical_count;
        attestation.high_issues = high_count;
        attestation.medium_issues = medium_count;
        attestation.low_issues = low_count;
        attestation.security_score = security_score;
        attestation.timestamp = Clock::get()?.unix_timestamp;
        attestation.bump = ctx.bumps.attestation_certificate;

        grant.state = GrantState::Certified;

        // Disburse remaining 75% balance of escrow to the auditor
        let remaining_balance = ctx.accounts.grant_vault.amount;
        let seeds = &[
            b"grant_vault",
            grant.to_account_info().key.as_ref(),
            &[grant.vault_bump],
        ];
        let signer = &[&seeds[..]];

        let cpi_accounts = Transfer {
            from: ctx.accounts.grant_vault.to_account_info(),
            to: ctx.accounts.auditor_token_account.to_account_info(),
            authority: ctx.accounts.grant_vault.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer,
        );
        token::transfer(cpi_ctx, remaining_balance)?;

        emit!(AuditCertified {
            grant: grant.key(),
            attestation: attestation.key(),
            report_hash,
            security_score,
            final_payout: remaining_balance,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// Account Contexts
// -----------------------------------------------------------------------------

#[derive(Accounts)]
#[instruction(project_id: String)]
pub struct InitializeGrant<'info> {
    #[account(
        init,
        payer = admin,
        space = 8 + GrantAccount::INIT_SPACE,
        seeds = [b"grant", project_authority.key().as_ref(), project_id.as_bytes()],
        bump
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        init,
        payer = admin,
        seeds = [b"grant_vault", grant_account.key().as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = grant_vault,
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    pub usdc_mint: Account<'info, Mint>,

    /// CHECK: The project's team lead / Colosseum verified authority
    pub project_authority: UncheckedAccount<'info>,

    /// CHECK: Vaultline Certified Lead Auditor wallet
    pub auditor: UncheckedAccount<'info>,

    #[account(mut)]
    pub admin: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

#[derive(Accounts)]
pub struct FundGrant<'info> {
    #[account(mut)]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_account.key().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = funder_token_account.mint == grant_account.usdc_mint @ GrantError::MintMismatch
    )]
    pub funder_token_account: Account<'info, TokenAccount>,

    pub funder: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct RecordScope<'info> {
    #[account(
        mut,
        has_one = auditor @ GrantError::UnauthorizedAuditor,
        has_one = usdc_mint @ GrantError::MintMismatch
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_account.key().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.mint == grant_account.usdc_mint @ GrantError::MintMismatch,
        constraint = auditor_token_account.owner == auditor.key() @ GrantError::InvalidAuditorTokenAccount
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    pub auditor: Signer<'info>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
pub struct FinalizeAuditAndAttest<'info> {
    #[account(
        mut,
        has_one = auditor @ GrantError::UnauthorizedAuditor,
        has_one = usdc_mint @ GrantError::MintMismatch
    )]
    pub grant_account: Account<'info, GrantAccount>,

    #[account(
        init,
        payer = auditor,
        space = 8 + AttestationCertificate::INIT_SPACE,
        seeds = [b"attestation", grant_account.key().as_ref()],
        bump
    )]
    pub attestation_certificate: Account<'info, AttestationCertificate>,

    #[account(
        mut,
        seeds = [b"grant_vault", grant_account.key().as_ref()],
        bump = grant_account.vault_bump
    )]
    pub grant_vault: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.mint == grant_account.usdc_mint @ GrantError::MintMismatch,
        constraint = auditor_token_account.owner == auditor.key() @ GrantError::InvalidAuditorTokenAccount
    )]
    pub auditor_token_account: Account<'info, TokenAccount>,

    #[account(mut)]
    pub auditor: Signer<'info>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub rent: Sysvar<'info, Rent>,
}

// -----------------------------------------------------------------------------
// Data Structures & Errors
// -----------------------------------------------------------------------------

#[account]
#[derive(InitSpace)]
pub struct GrantAccount {
    pub admin: Pubkey,
    pub project_authority: Pubkey,
    pub auditor: Pubkey,
    pub usdc_mint: Pubkey,
    pub grant_amount: u64,
    pub copay_amount: u64,
    pub total_funded: u64,
    #[max_len(64)]
    pub project_id: String,
    pub scope_commit_hash: [u8; 32],
    pub state: GrantState,
    pub bump: u8,
    pub vault_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct AttestationCertificate {
    pub grant: Pubkey,
    pub project_authority: Pubkey,
    pub auditor: Pubkey,
    pub report_hash: [u8; 32],
    pub critical_issues: u8,
    pub high_issues: u8,
    pub medium_issues: u8,
    pub low_issues: u8,
    pub security_score: u8,
    pub timestamp: i64,
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum GrantState {
    Created,
    PartiallyFunded,
    FullyFunded,
    ScopeLocked,
    Certified,
    Revoked,
}

#[error_code]
pub enum GrantError {
    #[msg("String length exceeds the maximum allowed boundary.")]
    StringTooLong,
    #[msg("Invalid funding or grant amount.")]
    InvalidAmount,
    #[msg("Arithmetic overflow occurred.")]
    MathOverflow,
    #[msg("State transition is disallowed in current status.")]
    InvalidStateTransition,
    #[msg("Token account mint mismatch.")]
    MintMismatch,
    #[msg("Signer is not the authorized auditor.")]
    UnauthorizedAuditor,
    #[msg("Invalid recipient token account for auditor.")]
    InvalidAuditorTokenAccount,
    #[msg("Project scope must specify between 1 and 8 program accounts.")]
    InvalidScopeList,
    #[msg("Security score must be an integer between 0 and 100.")]
    InvalidScore,
}

#[event]
pub struct GrantInitialized {
    pub grant: Pubkey,
    pub project: Pubkey,
    pub grant_amount: u64,
    pub copay_amount: u64,
}

#[event]
pub struct GrantFunded {
    pub grant: Pubkey,
    pub funder: Pubkey,
    pub amount: u64,
    pub total_funded: u64,
}

#[event]
pub struct ScopeLocked {
    pub grant: Pubkey,
    pub commit_hash: [u8; 32],
    pub tranche_released: u64,
}

#[event]
pub struct AuditCertified {
    pub grant: Pubkey,
    pub attestation: Pubkey,
    pub report_hash: [u8; 32],
    pub security_score: u8,
    pub final_payout: u64,
}