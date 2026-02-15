#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

SELF_HOST_RUNNER="$ROOT/wrela-v2/src/application/self_host_runner.wr"
REPRO_RUNNER="$ROOT/wrela-v2/src/application/repro_runner.wr"
STAGE_TOOLCHAIN_RUNNER="$ROOT/wrela-v2/src/application/stage_toolchain_runner.wr"
PARITY_COMMAND="$ROOT/wrela-v2/src/application/parity_command.wr"
BUILD_RUNNER="$ROOT/wrela-v2/src/application/build_runner.wr"
SELF_HOST_CONTRACT="$ROOT/wrela-v2/tests/contract/self_host_contract_test.wr"
REPRO_CONTRACT="$ROOT/wrela-v2/tests/contract/repro_contract_test.wr"
PARITY_CONTRACT="$ROOT/wrela-v2/tests/contract/parity_command_contract_test.wr"

required_files=(
  "$SELF_HOST_RUNNER"
  "$REPRO_RUNNER"
  "$STAGE_TOOLCHAIN_RUNNER"
  "$PARITY_COMMAND"
  "$BUILD_RUNNER"
  "$SELF_HOST_CONTRACT"
  "$REPRO_CONTRACT"
  "$PARITY_CONTRACT"
)

for required_file in "${required_files[@]}"; do
  if [[ ! -f "$required_file" ]]; then
    echo "v2 self-host bootstrap violation: missing required file $required_file" >&2
    exit 1
  fi
done

if ! rg -n "try_to_run_self_host_runner|self-host stage0 failed|self-host stage1 failed|self-host stage2 failed|self-host artifact missing" "$SELF_HOST_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: self-host runner symbols/errors missing in $SELF_HOST_RUNNER" >&2
  exit 1
fi

if ! rg -n "try_to_run_repro_runner|self-host stage2 failed|self-host reproducibility mismatch" "$REPRO_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: repro runner symbols/errors missing in $REPRO_RUNNER" >&2
  exit 1
fi

if ! rg -n "try_to_run_stage_toolchain|get_stage_exit_code_or_default|get_deterministic_stage_root" "$STAGE_TOOLCHAIN_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: stage toolchain runner symbols missing in $STAGE_TOOLCHAIN_RUNNER" >&2
  exit 1
fi

if ! rg -n "try_to_run_self_host_runner|parity: self-host reproducibility passed" "$PARITY_COMMAND" >/dev/null; then
  echo "v2 self-host bootstrap violation: parity command must run strict self-host gate in $PARITY_COMMAND" >&2
  exit 1
fi

if ! rg -n "WRELA_STAGE_HOST_BIN|create_cert_maps|m9-a" "$BUILD_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: build runner must emit stage-host artifact with m9 provenance in $BUILD_RUNNER" >&2
  exit 1
fi

if ! rg -n "test_self_host_runner_stage0_stage1_stage2_success|test_self_host_runner_fails_when_stage0_is_missing" "$SELF_HOST_CONTRACT" >/dev/null; then
  echo "v2 self-host bootstrap violation: required self-host contract scenarios missing in $SELF_HOST_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_repro_runner_passes_when_stage_hashes_match|test_repro_runner_fails_closed_on_mismatch|test_repro_runner_fails_when_stage1_artifact_missing" "$REPRO_CONTRACT" >/dev/null; then
  echo "v2 self-host bootstrap violation: required repro contract scenarios missing in $REPRO_CONTRACT" >&2
  exit 1
fi

if ! rg -n "test_parity_command_returns_two_when_self_host_lane_fails|test_parity_command_returns_zero_when_self_host_lane_passes" "$PARITY_CONTRACT" >/dev/null; then
  echo "v2 self-host bootstrap violation: required parity contract scenarios missing in $PARITY_CONTRACT" >&2
  exit 1
fi

if rg -n "== 0 or .*== 2|or parity_exit_code == 2" "$PARITY_CONTRACT" >/dev/null; then
  echo "v2 self-host bootstrap violation: permissive parity success assertion detected in $PARITY_CONTRACT" >&2
  exit 1
fi

if rg -n "stage0_bytes = __wr_bytes_from_string\(|stage1_bytes = stage0_bytes|stage2_exit_code = stage2_exit_code|stage1_exit_code = stage1_exit_code|stage0_exit_code = stage0_exit_code" "$SELF_HOST_RUNNER" "$REPRO_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: soft-fallback or suppressed stage exit handling detected" >&2
  exit 1
fi

if rg -n "fake_success|skip_stage|stage_bypass|mock_success" "$SELF_HOST_RUNNER" "$REPRO_RUNNER" "$STAGE_TOOLCHAIN_RUNNER" >/dev/null; then
  echo "v2 self-host bootstrap violation: self-host/repro bypass markers detected" >&2
  exit 1
fi

echo "v2 self-host bootstrap check passed"
