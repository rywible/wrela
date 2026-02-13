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

## Exception Process

If a change exceeds freeze scope, include a short exception note in the PR description:

- Why the change is needed now
- Why deferring is unsafe
- Which V2 readiness milestone it unblocks

## Enforcement

- Use `scripts/governance/capture_v1_baseline.sh` to refresh baseline artifacts.
- Use `scripts/governance/check_unsafe_allowlist.sh` to enforce unsafe quarantine boundaries.
