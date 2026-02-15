# V1 Freeze Policy

This repository is in V1 freeze mode to support V2 readiness and self-hosting preparation.

## Policy

- Allowed in V1:
  - Security fixes
  - Correctness fixes
  - Blockers for V2 readiness work
- Disallowed in V1 without explicit exception:
  - New language features
  - New runtime surface area not required by readiness scope
  - Refactors that are not tied to contract hardening or gating

## Required Checks

Every V1 change must satisfy:

1. `cargo test --workspace`
2. `cargo run -p wrela -- --help`
3. `cargo run -p wrela -- test apps/ledger-lite`
4. `scripts/governance/check_unsafe_allowlist.sh`
5. `scripts/governance/check_public_api_quarantine.sh`
6. `scripts/governance/check_v2_purity.sh`
7. `scripts/governance/check_v2_no_cheating.sh`
8. `scripts/governance/check_v2_platform_boundaries.sh`
9. `scripts/governance/check_v2_platform_contracts.sh`
10. `scripts/governance/check_v2_parity_bootstrap.sh`
11. `scripts/governance/check_v2_cli_bootstrap.sh`
12. `scripts/governance/check_v2_check_pipeline_bootstrap.sh`
13. `scripts/governance/check_phase0_abi_snapshot.sh`
14. `scripts/governance/check_phase0_surface_wiring.sh`
15. `scripts/governance/check_v2_guardrails.sh`

## Exception Process

If a change exceeds freeze scope, include a short exception note in the PR description:

- Why the change is needed now
- Why deferring is unsafe
- Which V2 readiness milestone it unblocks

## Enforcement

- Use `scripts/governance/capture_v1_baseline.sh` to refresh baseline artifacts.
- Use `scripts/governance/check_unsafe_allowlist.sh` to enforce unsafe quarantine boundaries.
