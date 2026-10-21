"""
Automated Solana Anchor Security Static Analyzer for Vaultline Colosseum Grants.
Scans Anchor smart contracts for critical vulnerability patterns:
1. Missing Signer Checks (Signer authorization bypass)
2. Account Ownership & Reinitialization Checks
3. Type Cosplay (Discriminator spoofing)
4. Arbitrary Cross-Program Invocations (CPI)
5. Unchecked Arithmetic Operations
"""

import re
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional


class Severity(Enum):
    CRITICAL = "CRITICAL"
    HIGH = "HIGH"
    MEDIUM = "MEDIUM"
    LOW = "LOW"
    INFORMATIONAL = "INFORMATIONAL"


@dataclass
class VulnerabilityFinding:
    rule_id: str
    title: str
    severity: Severity
    line_number: int
    matched_snippet: str
    description: str
    remediation: str


class SolanaAnchorSecurityScanner:
    """
    Static analysis engine purpose-built for Solana Rust/Anchor programs.
    """

    def __init__(self):
        self.findings: List[VulnerabilityFinding] = []

    def scan_source_code(self, filename: str, source_code: str) -> List[VulnerabilityFinding]:
        self.findings.clear()
        lines = source_code.splitlines()

        self._check_missing_signer(lines)
        self._check_unchecked_arithmetic(lines)
        self._check_arbitrary_cpi(lines)
        self._check_account_reinit(lines)
        self._check_raw_account_info(lines)

        return list(self.findings)

    def _check_missing_signer(self, lines: List[str]):
        """
        Detects accounts that should be Signers but are declared as AccountInfo or Account<'info, T>
        without Signer constraints in critical contexts.
        """
        for idx, line in enumerate(lines, 1):
            if "pub authority:" in line or "pub admin:" in line or "pub owner:" in line:
                if "Signer<'info>" not in line and "#[account(signer)]" not in lines[max(0, idx - 2)]:
                    self.findings.append(
                        VulnerabilityFinding(
                            rule_id="SEC-SOL-001",
                            title="Missing Signer Constraint on Privileged Authority",
                            severity=Severity.CRITICAL,
                            line_number=idx,
                            matched_snippet=line.strip(),
                            description="Privileged authority/admin field does not enforce Anchor Signer constraint.",
                            remediation="Use `pub authority: Signer<'info>` or enforce `#[account(signer)]`.",
                        )
                    )

    def _check_unchecked_arithmetic(self, lines: List[str]):
        """
        Detects naive arithmetic operations (+, -, *) that do not use checked_add, checked_sub, or checked_mul.
        """
        pattern = re.compile(r"(\w+)\s*([\+\-\*])=\s*(\w+);")
        for idx, line in enumerate(lines, 1):
            if pattern.search(line) and "checked_" not in line and "saturating_" not in line:
                # Exclude simple loop increments
                if "i += 1" in line or "idx += 1" in line:
                    continue
                self.findings.append(
                    VulnerabilityFinding(
                        rule_id="SEC-SOL-002",
                        title="Unchecked Integer Arithmetic",
                        severity=Severity.HIGH,
                        line_number=idx,
                        matched_snippet=line.strip(),
                        description="Direct arithmetic operation susceptible to integer overflow/underflow.",
                        remediation="Replace with `.checked_add()`, `.checked_sub()`, or `.checked_mul()?.ok_or(...)`.",
                    )
                )

    def _check_arbitrary_cpi(self, lines: List[str]):
        """
        Detects invoke / invoke_signed calls where program_id is passed directly from an unvalidated AccountInfo.
        """
        for idx, line in enumerate(lines, 1):
            if "invoke(" in line or "invoke_signed(" in line:
                # Check previous lines for program validation
                context = "\n".join(lines[max(0, idx - 5) : idx])
                if "constraint = " not in context and "key() ==" not in context:
                    self.findings.append(
                        VulnerabilityFinding(
                            rule_id="SEC-SOL-003",
                            title="Arbitrary Cross-Program Invocation (CPI) Target",
                            severity=Severity.CRITICAL,
                            line_number=idx,
                            matched_snippet=line.strip(),
                            description="CPI execution target is not strictly validated against expected program address.",
                            remediation="Add `require_keys_eq!(target_program.key(), expected_program::ID);` before invoke.",
                        )
                    )

    def _check_account_reinit(self, lines: List[str]):
        """
        Detects init without proper seed derivation or re-initialization guards.
        """
        for idx, line in enumerate(lines, 1):
            if "#[account(init" in line:
                context = "\n".join(lines[max(0, idx - 1) : min(len(lines), idx + 3)])
                if "seeds" not in context and "realloc" not in context:
                    self.findings.append(
                        VulnerabilityFinding(
                            rule_id="SEC-SOL-004",
                            title="Unconstrained Account Initialization",
                            severity=Severity.MEDIUM,
                            line_number=idx,
                            matched_snippet=line.strip(),
                            description="Account initialized without PDA seeds constraint.",
                            remediation="Bind initialized accounts to deterministic PDA seeds via `seeds = [...]`.",
                        )
                    )

    def _check_raw_account_info(self, lines: List[str]):
        """
        Detects usage of raw AccountInfo without discriminator verification.
        """
        for idx, line in enumerate(lines, 1):
            if "AccountInfo<'info>" in line and "/// CHECK:" not in lines[max(0, idx - 2)]:
                self.findings.append(
                    VulnerabilityFinding(
                        rule_id="SEC-SOL-005",
                        title="Unchecked Raw AccountInfo Usage",
                        severity=Severity.MEDIUM,
                        line_number=idx,
                        matched_snippet=line.strip(),
                        description="Raw AccountInfo used without Anchor safety doc comment `/// CHECK:` or manual discriminator check.",
                        remediation="Use typed `Account<'info, T>` or add explicit safety comment and ownership check.",
                    )
                )
