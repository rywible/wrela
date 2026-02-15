#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

"$ROOT/scripts/governance/check_v2_purity.sh"
"$ROOT/scripts/governance/check_v2_no_cheating.sh"
"$ROOT/scripts/governance/check_v2_platform_boundaries.sh"
"$ROOT/scripts/governance/check_v2_platform_contracts.sh"
"$ROOT/scripts/governance/check_v2_parity_bootstrap.sh"
"$ROOT/scripts/governance/check_v2_cli_bootstrap.sh"
"$ROOT/scripts/governance/check_v2_check_pipeline_bootstrap.sh"
"$ROOT/scripts/governance/check_v2_self_host_bootstrap.sh"
"$ROOT/scripts/governance/check_v2_m10_cutover.sh"
"$ROOT/scripts/governance/check_phase0_abi_snapshot.sh"
"$ROOT/scripts/governance/check_phase0_surface_wiring.sh"

echo "v2 guardrails check passed"
