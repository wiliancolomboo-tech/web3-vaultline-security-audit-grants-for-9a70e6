use anchor_lang::prelude::*;
use anchor_spl::token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked};

declare_id!("VLinE6bK8c6QeS9f7yZc9YdM7XbQ8mQnL1V2A3B4C5D");

pub const GRANT_TOTAL_ALLOCATION: u64 = 4_000_000_000; // 4,000 USDC (6 decimals)
pub const TRANCHE_1_AMOUNT: u64 = 1_000_000_000;        // 25%: Scope & Threat Model
pub const TRANCHE_2_AMOUNT: u64 = 2_000_000_000;        // 50%: Draft Vulnerability Report
pub const TRANCHE_3_AMOUNT: u64 = 1_000_000_000;        // 25%: Remediation & Sign-off

#[program]
pub mod vaultline_grants {
    use super::*;

    /// Initializes the global Vaultline Grant Pool treasury.
    pub fn initialize_pool(
        ctx: Context<InitializePool>,
        pool_bump: u8,
    ) -> Result<()> {
        let pool = &mut ctx.accounts.grant_pool;
        pool.admin = ctx.accounts.admin.key();
        pool.usdc_mint = ctx.accounts.usdc_mint.key();
        pool.total_allocated = 0;
        pool.total_disbursed = 0;
        pool.grant_count = 0;
        pool.bump = pool_bump;
        Ok(())
    }

    /// Allocates an escrow grant of 4,000 USDC for a verified Colosseum project.
    pub fn create_audit_grant(
        ctx: Context<CreateAuditGrant>,
        project_id: [u8; 32],
        project_name: String,
        repo_commit_hash: [u8; 32],
    ) -> Result<()> {
        require!(project_name.len() <= 64, VaultlineError::NameTooLong);

        let grant = &mut ctx.accounts.audit_grant;
        let pool = &mut ctx.accounts.grant_pool;

        grant.project_id = project_id;
        grant.project_name = project_name;
        grant.applicant = ctx.accounts.applicant.key();
        grant.auditor = ctx.accounts.auditor.key();
        grant.repo_commit_hash = repo_commit_hash;
        grant.status = GrantStatus::Initialized;
        grant.total_allocated = GRANT_TOTAL_ALLOCATION;
        grant.total_disbursed = 0;
        grant.current_milestone = 0;
        grant.bump = ctx.bumps.audit_grant;
        grant.vault_bump = ctx.bumps.grant_vault;

        // Initialize 3 programmatic milestones
        grant.milestones[0] = Milestone {
            target_amount: TRANCHE_1_AMOUNT,
            state: MilestoneState::Pending,
            deliverable_hash: [0u8; 32],
            completion_timestamp: 0,
        };
        grant.milestones[1] = Milestone {
            target_amount: TRANCHE_2_AMOUNT,
            state: MilestoneState::Pending,
            deliverable_hash: [0u8; 32],
            completion_timestamp: 0,
        };
        grant.milestones[2] = Milestone {
            target_amount: TRANCHE_3_AMOUNT,
            state: MilestoneState::Pending,
            deliverable_hash: [0u8; 32],
            completion_timestamp: 0,
        };

        // Transfer 4,000 USDC from funder to grant escrow vault
        let decimals = ctx.accounts.usdc_mint.decimals;
        token_interface::transfer_checked(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.funder_token_account.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.grant_vault.to_account_info(),
                    authority: ctx.accounts.funder.to_account_info(),
                },
            ),
            GRANT_TOTAL_ALLOCATION,
            decimals,
        )?;

        pool.total_allocated = pool
            .total_allocated
            .checked_add(GRANT_TOTAL_ALLOCATION)
            .ok_or(VaultlineError::MathOverflow)?;
        pool.grant_count = pool
            .grant_count
            .checked_add(1)
            .ok_or(VaultlineError::MathOverflow)?;

        emit!(GrantCreatedEvent {
            project_id,
            applicant: grant.applicant,
            auditor: grant.auditor,
            total_allocated: GRANT_TOTAL_ALLOCATION,
        });

        Ok(())
    }

    /// Auditor commits a cryptographic artifact hash for the active milestone deliverable.
    pub fn submit_milestone_deliverable(
        ctx: Context<SubmitDeliverable>,
        milestone_index: u8,
        deliverable_hash: [u8; 32],
    ) -> Result<()> {
        let grant = &mut ctx.accounts.audit_grant;
        let index = milestone_index as usize;

        require!(index < 3, VaultlineError::InvalidMilestoneIndex);
        require!(
            grant.status != GrantStatus::Cancelled && grant.status != GrantStatus::Completed,
            VaultlineError::GrantInactive
        );
        require!(
            grant.current_milestone == milestone_index,
            VaultlineError::MilestoneSequenceViolation
        );

        let milestone = &mut grant.milestones[index];
        require!(
            milestone.state == MilestoneState::Pending,
            VaultlineError::DeliverableAlreadySubmitted
        );

        milestone.deliverable_hash = deliverable_hash;
        milestone.state = MilestoneState::Submitted;

        if grant.status == GrantStatus::Initialized {
            grant.status = GrantStatus::InProgress;
        }

        emit!(MilestoneSubmittedEvent {
            project_id: grant.project_id,
            milestone_index,
            deliverable_hash,
        });

        Ok(())
    }

    /// Admin / Grant Oracle approves deliverable and releases USDC tranche directly to the Auditor.
    pub fn approve_and_disburse(
        ctx: Context<ApproveAndDisburse>,
        milestone_index: u8,
    ) -> Result<()> {
        let grant = &mut ctx.accounts.audit_grant;
        let pool = &mut ctx.accounts.grant_pool;
        let index = milestone_index as usize;

        require!(index < 3, VaultlineError::InvalidMilestoneIndex);
        require!(
            grant.current_milestone == milestone_index,
            VaultlineError::MilestoneSequenceViolation
        );

        let milestone = &mut grant.milestones[index];
        require!(
            milestone.state == MilestoneState::Submitted,
            VaultlineError::MilestoneNotSubmitted
        );

        let payout_amount = milestone.target_amount;
        let clock = Clock::get()?;

        milestone.state = MilestoneState::Approved;
        milestone.completion_timestamp = clock.unix_timestamp;

        grant.total_disbursed = grant
            .total_disbursed
            .checked_add(payout_amount)
            .ok_or(VaultlineError::MathOverflow)?;

        pool.total_disbursed = pool
            .total_disbursed
            .checked_add(payout_amount)
            .ok_or(VaultlineError::MathOverflow)?;

        // Execute CPI transfer from PDA escrow vault to Auditor ATA
        let project_id_bytes = grant.project_id;
        let seeds = &[
            b"grant_vault",
            project_id_bytes.as_ref(),
            &[grant.vault_bump],
        ];
        let signer = &[&seeds[..]];

        token_interface::transfer_checked(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                TransferChecked {
                    from: ctx.accounts.grant_vault.to_account_info(),
                    mint: ctx.accounts.usdc_mint.to_account_info(),
                    to: ctx.accounts.auditor_token_account.to_account_info(),
                    authority: ctx.accounts.grant_vault.to_account_info(),
                },
                signer,
            ),
            payout_amount,
            ctx.accounts.usdc_mint.decimals,
        )?;

        if index == 2 {
            grant.status = GrantStatus::Completed;
            emit!(GrantCompletedEvent {
                project_id: grant.project_id,
                total_disbursed: grant.total_disbursed,
            });
        } else {
            grant.current_milestone = milestone_index + 1;
        }

        emit!(MilestoneDisbursedEvent {
            project_id: grant.project_id,
            milestone_index,
            amount_disbursed: payout_amount,
            auditor: grant.auditor,
        });

        Ok(())
    }

    /// Admin can cancel a defaulted project grant and reclaim unreleased USDC to the treasury.
    pub fn cancel_defaulted_grant(ctx: Context<CancelDefaultedGrant>) -> Result<()> {
        let grant = &mut ctx.accounts.audit_grant;
        require!(grant.status != GrantStatus::Completed, VaultlineError::GrantAlreadyCompleted);

        let remaining_balance = ctx.accounts.grant_vault.amount;
        if remaining_balance > 0 {
            let project_id_bytes = grant.project_id;
            let seeds = &[
                b"grant_vault",
                project_id_bytes.as_ref(),
                &[grant.vault_bump],
            ];
            let signer = &[&seeds[..]];

            token_interface::transfer_checked(
                CpiContext::new_with_signer(
                    ctx.accounts.token_program.to_account_info(),
                    TransferChecked {
                        from: ctx.accounts.grant_vault.to_account_info(),
                        mint: ctx.accounts.usdc_mint.to_account_info(),
                        to: ctx.accounts.admin_token_account.to_account_info(),
                        authority: ctx.accounts.grant_vault.to_account_info(),
                    },
                    signer,
                ),
                remaining_balance,
                ctx.accounts.usdc_mint.decimals,
            )?;
        }

        grant.status = GrantStatus::Cancelled;

        emit!(GrantCancelledEvent {
            project_id: grant.project_id,
            reclaimed_amount: remaining_balance,
        });

        Ok(())
    }
}

// -----------------------------------------------------------------------------
// ACCOUNT STRUCTS & STATES
// -----------------------------------------------------------------------------

#[account]
#[derive(Default)]
pub struct GrantPool {
    pub admin: Pubkey,          // 32
    pub usdc_mint: Pubkey,      // 32
    pub total_allocated: u64,   // 8
    pub total_disbursed: u64,   // 8
    pub grant_count: u64,       // 8
    pub bump: u8,               // 1
}

impl GrantPool {
    pub const LEN: usize = 8 + 32 + 32 + 8 + 8 + 8 + 1;
}

#[account]
pub struct AuditGrant {
    pub project_id: [u8; 32],       // 32
    pub project_name: String,        // 4 + 64
    pub applicant: Pubkey,          // 32
    pub auditor: Pubkey,            // 32
    pub repo_commit_hash: [u8; 32], // 32
    pub status: GrantStatus,         // 1
    pub total_allocated: u64,       // 8
    pub total_disbursed: u64,       // 8
    pub current_milestone: u8,      // 1
    pub milestones: [Milestone; 3],  // 3 * 73 = 219
    pub bump: u8,                   // 1
    pub vault_bump: u8,             // 1
}

impl AuditGrant {
    pub const LEN: usize = 8 + 32 + (4 + 64) + 32 + 32 + 32 + 1 + 8 + 8 + 1 + (3 * Milestone::LEN) + 1 + 1;
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum GrantStatus {
    Initialized,
    InProgress,
    Completed,
    Cancelled,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq)]
pub enum MilestoneState {
    Pending,
    Submitted,
    Approved,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct Milestone {
    pub target_amount: u64,           // 8
    pub state: MilestoneState,        // 1
    pub deliverable_hash: [u8; 32],   // 32
    pub completion_timestamp: i64,    // 8
}

impl Milestone {
    pub const LEN: usize = 8 + 1 + 32 + 8;
}

// -----------------------------------------------------------------------------
// CONTEXTS
// -----------------------------------------------------------------------------

#[derive(Accounts)]
pub struct InitializePool<'info> {
    #[account(
        init,
        payer = admin,
        space = GrantPool::LEN,
        seeds = [b"grant_pool"],
        bump
    )]
    pub grant_pool: Account<'info, GrantPool>,
    pub usdc_mint: InterfaceAccount<'info, Mint>,
    #[account(mut)]
    pub admin: Signer<'info>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(project_id: [u8; 32])]
pub struct CreateAuditGrant<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = grant_pool.bump,
        has_one = admin,
    )]
    pub grant_pool: Account<'info, GrantPool>,

    #[account(
        init,
        payer = funder,
        space = AuditGrant::LEN,
        seeds = [b"audit_grant", project_id.as_ref()],
        bump
    )]
    pub audit_grant: Account<'info, AuditGrant>,

    #[account(
        init,
        payer = funder,
        seeds = [b"grant_vault", project_id.as_ref()],
        bump,
        token::mint = usdc_mint,
        token::authority = grant_vault,
    )]
    pub grant_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(address = grant_pool.usdc_mint)]
    pub usdc_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        constraint = funder_token_account.mint == usdc_mint.key(),
        constraint = funder_token_account.owner == funder.key()
    )]
    pub funder_token_account: InterfaceAccount<'info, TokenAccount>,

    pub admin: Signer<'info>,
    #[account(mut)]
    pub funder: Signer<'info>,
    /// CHECK: Colosseum participant wallet
    pub applicant: AccountInfo<'info>,
    /// CHECK: Audit partner wallet
    pub auditor: AccountInfo<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct SubmitDeliverable<'info> {
    #[account(
        mut,
        seeds = [b"audit_grant", audit_grant.project_id.as_ref()],
        bump = audit_grant.bump,
        has_one = auditor,
    )]
    pub audit_grant: Account<'info, AuditGrant>,
    pub auditor: Signer<'info>,
}

#[derive(Accounts)]
#[instruction(milestone_index: u8)]
pub struct ApproveAndDisburse<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = grant_pool.bump,
        has_one = admin,
        has_one = usdc_mint,
    )]
    pub grant_pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"audit_grant", audit_grant.project_id.as_ref()],
        bump = audit_grant.bump,
    )]
    pub audit_grant: Account<'info, AuditGrant>,

    #[account(
        mut,
        seeds = [b"grant_vault", audit_grant.project_id.as_ref()],
        bump = audit_grant.vault_bump,
        token::mint = usdc_mint,
        token::authority = grant_vault,
    )]
    pub grant_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = auditor_token_account.owner == audit_grant.auditor,
        constraint = auditor_token_account.mint == usdc_mint.key(),
    )]
    pub auditor_token_account: InterfaceAccount<'info, TokenAccount>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,
    pub admin: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

#[derive(Accounts)]
pub struct CancelDefaultedGrant<'info> {
    #[account(
        mut,
        seeds = [b"grant_pool"],
        bump = grant_pool.bump,
        has_one = admin,
        has_one = usdc_mint,
    )]
    pub grant_pool: Account<'info, GrantPool>,

    #[account(
        mut,
        seeds = [b"audit_grant", audit_grant.project_id.as_ref()],
        bump = audit_grant.bump,
    )]
    pub audit_grant: Account<'info, AuditGrant>,

    #[account(
        mut,
        seeds = [b"grant_vault", audit_grant.project_id.as_ref()],
        bump = audit_grant.vault_bump,
        token::mint = usdc_mint,
        token::authority = grant_vault,
    )]
    pub grant_vault: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        constraint = admin_token_account.owner == admin.key(),
        constraint = admin_token_account.mint == usdc_mint.key(),
    )]
    pub admin_token_account: InterfaceAccount<'info, TokenAccount>,

    pub usdc_mint: InterfaceAccount<'info, Mint>,
    pub admin: Signer<'info>,
    pub token_program: Interface<'info, TokenInterface>,
}

// -----------------------------------------------------------------------------
// EVENTS & ERRORS
// -----------------------------------------------------------------------------

#[event]
pub struct GrantCreatedEvent {
    pub project_id: [u8; 32],
    pub applicant: Pubkey,
    pub auditor: Pubkey,
    pub total_allocated: u64,
}

#[event]
pub struct MilestoneSubmittedEvent {
    pub project_id: [u8; 32],
    pub milestone_index: u8,
    pub deliverable_hash: [u8; 32],
}

#[event]
pub struct MilestoneDisbursedEvent {
    pub project_id: [u8; 32],
    pub milestone_index: u8,
    pub amount_disbursed: u64,
    pub auditor: Pubkey,
}

#[event]
pub struct GrantCompletedEvent {
    pub project_id: [u8; 32],
    pub total_disbursed: u64,
}

#[event]
pub struct GrantCancelledEvent {
    pub project_id: [u8; 32],
    pub reclaimed_amount: u64,
}

#[error_code]
pub enum VaultlineError {
    #[msg("Project name exceeds 64 bytes.")]
    NameTooLong,
    #[msg("Invalid milestone index provided.")]
    InvalidMilestoneIndex,
    #[msg("Milestone deliverables must be completed sequentially.")]
    MilestoneSequenceViolation,
    #[msg("Deliverable for this milestone was already submitted.")]
    DeliverableAlreadySubmitted,
    #[msg("Milestone deliverable has not been submitted yet.")]
    MilestoneNotSubmitted,
    #[msg("This grant is no longer active.")]
    GrantInactive,
    #[msg("Grant is already marked completed.")]
    GrantAlreadyCompleted,
    #[msg("Arithmetic overflow occurred.")]
    MathOverflow,
}