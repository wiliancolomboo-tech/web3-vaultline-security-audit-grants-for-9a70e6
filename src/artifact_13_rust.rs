use anchor_lang::prelude::*;

pub mod errors;
pub mod instructions;
pub mod state;

use instructions::*;

declare_id!("V1tLN8Z3jQdKzVw7dM5Lq2YkP8g1mE4sN6uRxT9aBC1");

#[program]
pub mod vaultline_grants {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        instructions::initialize::handler(ctx)
    }

    pub fn apply_grant(
        ctx: Context<ApplyGrant>,
        grant_id: [u8; 16],
        allocated_amount: u64,
        sla_duration_seconds: i64,
    ) -> Result<()> {
        instructions::apply_grant::handler(ctx, grant_id, allocated_amount, sla_duration_seconds)
    }

    pub fn fund_grant(ctx: Context<FundGrant>) -> Result<()> {
        instructions::fund_grant::handler(ctx)
    }

    pub fn submit_report(
        ctx: Context<SubmitReport>,
        report_digest: [u8; 32],
    ) -> Result<()> {
        instructions::submit_report::handler(ctx, report_digest)
    }

    pub fn release_milestone_1(ctx: Context<ReleaseMilestone>) -> Result<()> {
        instructions::release_milestone::handle_milestone_1(ctx)
    }

    pub fn release_milestone_2(
        ctx: Context<ReleaseMilestone>,
        remediation_digest: [u8; 32],
    ) -> Result<()> {
        instructions::release_milestone::handle_milestone_2(ctx, remediation_digest)
    }

    pub fn emergency_clawback(ctx: Context<EmergencyClawback>) -> Result<()> {
        instructions::emergency_clawback::handler(ctx)
    }
}