# V2 No-Cheating Policy

These are hard constraints, not suggestions.

1. No shortcuts.
2. No delegation to Rust or V1 in production v2 code paths.
3. No fake pass conditions for parity, certification, linker, or runtime behavior.
4. No hidden bypass flags that disable required checks.
5. No platform lock-in in core layers.
6. No placeholder tests or placeholder production paths in `wrela-v2/src` and `wrela-v2/tests`.
7. No direct `__wr_env_get` / `__wr_process_argv` intrinsic usage in v2 app/domain code (use `host/process` wrappers).

Enforcement lives in:

- `scripts/governance/check_v2_purity.sh`
- `scripts/governance/check_v2_no_cheating.sh`
- `scripts/governance/check_v2_platform_boundaries.sh`
- `scripts/governance/check_v2_platform_contracts.sh`
- `scripts/governance/check_v2_parity_bootstrap.sh`
- `scripts/governance/check_v2_cli_bootstrap.sh`
- `scripts/governance/check_v2_check_pipeline_bootstrap.sh`
- `scripts/governance/check_phase0_abi_snapshot.sh`
- `scripts/governance/check_phase0_surface_wiring.sh`
- `scripts/governance/check_v2_guardrails.sh`
