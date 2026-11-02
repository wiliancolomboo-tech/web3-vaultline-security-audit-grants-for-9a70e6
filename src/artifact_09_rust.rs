use anchor_lang::prelude::*;
use crate::errors::VaultlineError;
use crate::state::*;

#[derive(Accounts)]
pub struct SubmitReport<'info> {
    #[account(
        mut,
        seeds = [b"grant", grant.grant_id.as_ref()],
        bump = grant.bump,
        has_one = auditor
    )]
    pub grant: Account<'info, AuditGrant>,

    pub auditor: Signer<'info>,
}

pub fn handler(
    ctx: Context<SubmitReport>,
    report_digest: [u8; 32],
) -> Result<()> {
    let grant = &mut ctx.accounts.grant;
    require!(
        grant.status == GrantStatus::Funded,
        VaultlineError::InvalidGrantStatus
    );

    let clock = Clock::get()?;
    require!(
        clock.unix_timestamp <= grant.sla_deadline_ts,
        VaultlineError::SlaDeadlineExceeded
    );

    require!(report_digest != [0u8; 32], VaultlineError::InvalidReportDigest);

    grant.report_digest = report_digest;
    grant.status = GrantStatus::InReview;
    Ok(())
}