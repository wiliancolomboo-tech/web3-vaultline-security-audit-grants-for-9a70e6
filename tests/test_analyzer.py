"""
Test Suite for Solana Anchor Security Static Analyzer.
Validates detection of Missing Signers, Unchecked Arithmetic, Arbitrary CPIs, and Raw AccountInfo risks.
"""

import os
import sys
from pathlib import Path

PROJECT_ROOT = Path(__file__).resolve().parent.parent.parent.parent
if str(PROJECT_ROOT) not in sys.path:
    sys.path.insert(0, str(PROJECT_ROOT))

MODULE_ROOT = Path(__file__).resolve().parent.parent
if str(MODULE_ROOT) not in sys.path:
    sys.path.insert(0, str(MODULE_ROOT))

import pytest
try:
    from deliverables.superteam_vaultline_security_grant.audit_engine.static_analyzer import (
        SolanaAnchorSecurityScanner,
        Severity,
    )
except ImportError:
    from audit_engine.static_analyzer import (
        SolanaAnchorSecurityScanner,
        Severity,
    )

VULNERABLE_ANCHOR_PROGRAM = """
use anchor_lang::prelude::*;

declare_id!("Fg6PaFpoGXkYsidMpWTK6W2BeZ7FEfcYkg476zPFsLnS");

#[program]
pub mod vulnerable_vault {
    use super::*;

    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        let vault = &mut ctx.accounts.vault;
        vault.total_balance -= amount;

        let cpi_program = ctx.accounts.token_program.to_account_info();
        anchor_lang::solana_program::program::invoke(
            &instruction,
            &[vault.to_account_info(), ctx.accounts.recipient.to_account_info()]
        )?;
        Ok(())
    }
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub vault: Account<'info, Vault>,
    pub authority: AccountInfo<'info>,
    pub recipient: AccountInfo<'info>,
    pub token_program: AccountInfo<'info>,
}

#[account]
pub struct Vault {
    pub total_balance: u64,
}
"""


def test_missing_signer_detection():
    scanner = SolanaAnchorSecurityScanner()
    findings = scanner.scan_source_code("vault.rs", VULNERABLE_ANCHOR_PROGRAM)

    rule_ids = [f.rule_id for f in findings]
    assert "SEC-SOL-001" in rule_ids, "Deveria detectar autoridade sem Signer constraint"

    signer_finding = next(f for f in findings if f.rule_id == "SEC-SOL-001")
    assert signer_finding.severity == Severity.CRITICAL


def test_unchecked_arithmetic_detection():
    scanner = SolanaAnchorSecurityScanner()
    findings = scanner.scan_source_code("vault.rs", VULNERABLE_ANCHOR_PROGRAM)

    rule_ids = [f.rule_id for f in findings]
    assert "SEC-SOL-002" in rule_ids, "Deveria detectar subtracao sem checked_sub"


def test_arbitrary_cpi_detection():
    scanner = SolanaAnchorSecurityScanner()
    findings = scanner.scan_source_code("vault.rs", VULNERABLE_ANCHOR_PROGRAM)

    rule_ids = [f.rule_id for f in findings]
    assert "SEC-SOL-003" in rule_ids, "Deveria detectar CPI sem validacao de program_id"


def test_clean_anchor_program_zero_criticals():
    clean_program = """
    #[derive(Accounts)]
    pub struct CleanWithdraw<'info> {
        #[account(mut, has_one = authority)]
        pub vault: Account<'info, Vault>,
        pub authority: Signer<'info>,
    }
    """
    scanner = SolanaAnchorSecurityScanner()
    findings = scanner.scan_source_code("clean.rs", clean_program)
    criticals = [f for f in findings if f.severity == Severity.CRITICAL]
    assert len(criticals) == 0


if __name__ == "__main__":
    test_missing_signer_detection()
    test_unchecked_arithmetic_detection()
    test_arbitrary_cpi_detection()
    test_clean_anchor_program_zero_criticals()
    print("Todos os testes unitários do Aegis-Sol Static Analyzer foram aprovados com 100% de sucesso!")
