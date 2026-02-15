#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CLI="$ROOT/wrela-v2/src/application/cli.wr"
MAIN="$ROOT/wrela-v2/src/main.wr"
CONTRACT="$ROOT/wrela-v2/tests/contract/cli_bootstrap_contract_test.wr"

COMMAND_FILES=(
  "$ROOT/wrela-v2/src/application/init_command.wr"
  "$ROOT/wrela-v2/src/application/update_command.wr"
  "$ROOT/wrela-v2/src/application/check_command.wr"
  "$ROOT/wrela-v2/src/application/build_command.wr"
  "$ROOT/wrela-v2/src/application/compile_command.wr"
  "$ROOT/wrela-v2/src/application/verify_cert_command.wr"
  "$ROOT/wrela-v2/src/application/run_command.wr"
  "$ROOT/wrela-v2/src/application/dev_command.wr"
  "$ROOT/wrela-v2/src/application/test_command.wr"
  "$ROOT/wrela-v2/src/application/perf_command.wr"
  "$ROOT/wrela-v2/src/application/perfcmp_command.wr"
  "$ROOT/wrela-v2/src/application/matrix_command.wr"
  "$ROOT/wrela-v2/src/application/parity_command.wr"
  "$ROOT/wrela-v2/src/application/build_runner.wr"
  "$ROOT/wrela-v2/src/application/test_runner.wr"
  "$ROOT/wrela-v2/src/application/update_runner.wr"
  "$ROOT/wrela-v2/src/application/verify_cert_runner.wr"
  "$ROOT/wrela-v2/src/application/dev_runner.wr"
  "$ROOT/wrela-v2/src/application/perf_runner.wr"
  "$ROOT/wrela-v2/src/application/perfcmp_runner.wr"
  "$ROOT/wrela-v2/src/application/matrix_runner.wr"
  "$ROOT/wrela-v2/src/domain/cert/hash.wr"
  "$ROOT/wrela-v2/src/domain/cert/model.wr"
  "$ROOT/wrela-v2/src/domain/cert/validate.wr"
)

CONTRACT_FILES=(
  "$ROOT/wrela-v2/tests/contract/cli_bootstrap_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/update_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/build_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/compile_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/verify_cert_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/run_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/dev_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/test_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/perf_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/perfcmp_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/matrix_command_contract_test.wr"
  "$ROOT/wrela-v2/tests/contract/m7_command_runtime_coverage_contract_test.wr"
)

if [[ ! -f "$CLI" ]]; then
  echo "missing v2 cli module: $CLI" >&2
  exit 1
fi

if [[ ! -f "$MAIN" ]]; then
  echo "missing v2 entrypoint: $MAIN" >&2
  exit 1
fi

if [[ ! -f "$CONTRACT" ]]; then
  echo "missing v2 cli contract file: $CONTRACT" >&2
  exit 1
fi

for command_file in "${COMMAND_FILES[@]}"; do
  if [[ ! -f "$command_file" ]]; then
    echo "missing v2 command module: $command_file" >&2
    exit 1
  fi
done

for contract_file in "${CONTRACT_FILES[@]}"; do
  if [[ ! -f "$contract_file" ]]; then
    echo "missing v2 command contract file: $contract_file" >&2
    exit 1
  fi
done

required_cli_symbols=(
  "run_cli"
  "print_usage"
  "run_init_command"
  "run_update_command"
  "run_check_command"
  "run_build_command"
  "run_compile_command"
  "run_verify_cert_command"
  "run_run_command"
  "run_dev_command"
  "run_test_command"
  "run_perf_command"
  "run_perfcmp_command"
  "run_matrix_command"
  "run_parity_command"
)

for symbol in "${required_cli_symbols[@]}"; do
  if ! rg -n "$symbol" "$CLI" >/dev/null; then
    echo "v2 cli bootstrap violation: missing '$symbol' in $CLI" >&2
    exit 1
  fi
done

required_dispatches=(
  'command_value == "init"'
  'command_value == "update"'
  'command_value == "check"'
  'command_value == "build"'
  'command_value == "compile"'
  'command_value == "verify-cert"'
  'command_value == "run"'
  'command_value == "dev"'
  'command_value == "test"'
  'command_value == "perf"'
  'command_value == "perfcmp"'
  'command_value == "matrix"'
  'command_value == "parity"'
)

for dispatch in "${required_dispatches[@]}"; do
  if ! rg -n "$dispatch" "$CLI" >/dev/null; then
    echo "v2 cli bootstrap violation: dispatch '$dispatch' missing in $CLI" >&2
    exit 1
  fi
done

if ! rg -n 'command_value == "help"|command_value == "--help"|command_value == "-h"' "$CLI" >/dev/null; then
  echo "v2 cli bootstrap violation: help dispatch missing in $CLI" >&2
  exit 1
fi

if ! rg -n "run_cli" "$MAIN" >/dev/null; then
  echo "v2 cli bootstrap violation: run_cli is not wired in $MAIN" >&2
  exit 1
fi

if ! rg -n "try_to_build_executable" "$ROOT/wrela-v2/src/application/build_command.wr" "$ROOT/wrela-v2/src/application/run_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: core build/run commands must use build runner" >&2
  exit 1
fi

if ! rg -n "try_to_run_tests" "$ROOT/wrela-v2/src/application/test_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: test command must use test runner" >&2
  exit 1
fi

if ! rg -n "try_to_run_update_runner" "$ROOT/wrela-v2/src/application/update_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: update command must use update runner" >&2
  exit 1
fi
if ! rg -n "try_to_run_verify_cert_runner" "$ROOT/wrela-v2/src/application/verify_cert_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: verify-cert command must use verify-cert runner" >&2
  exit 1
fi
if ! rg -n "try_to_run_dev_runner" "$ROOT/wrela-v2/src/application/dev_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: dev command must use dev runner" >&2
  exit 1
fi
if ! rg -n "try_to_run_perf_runner" "$ROOT/wrela-v2/src/application/perf_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perf command must use perf runner" >&2
  exit 1
fi
if ! rg -n "try_to_run_perfcmp_runner" "$ROOT/wrela-v2/src/application/perfcmp_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perfcmp command must use perfcmp runner" >&2
  exit 1
fi
if ! rg -n "try_to_run_matrix_runner" "$ROOT/wrela-v2/src/application/matrix_command.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: matrix command must use matrix runner" >&2
  exit 1
fi
if ! rg -n "timestamp_ns|p50_ns|p95_ns|p99_ns" "$ROOT/wrela-v2/src/application/perf_runner.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perf runner must emit deterministic aggregate fields including timestamp" >&2
  exit 1
fi
if ! rg -n "required_baseline_key_texts|status=regression|threshold_percent" "$ROOT/wrela-v2/src/application/perfcmp_runner.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perfcmp runner must validate baseline schema and emit regression status report" >&2
  exit 1
fi
if ! rg -n "cells=1,3,5|failures_total|cell_1_status|cell_3_status|cell_5_status" "$ROOT/wrela-v2/src/application/matrix_runner.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: matrix runner must emit deterministic cell ordering and status fields" >&2
  exit 1
fi
if ! rg -n "create_cert_maps|get_cert_text|get_cert_hash_text|get_cert_hash_text_from_byte_lists" "$ROOT/wrela-v2/src/application/build_runner.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: build runner must emit canonical cert artifact" >&2
  exit 1
fi
if ! rg -n "try_to_validate_cert_text" "$ROOT/wrela-v2/src/application/verify_cert_runner.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: verify-cert runner must use domain cert validation" >&2
  exit 1
fi

if rg -n "command 'build' is not implemented in v2 yet|command 'compile' is not implemented in v2 yet|command 'run' is not implemented in v2 yet|command 'test' is not implemented in v2 yet|command 'update' is not implemented in v2 yet|command 'verify-cert' is not implemented in v2 yet|command 'dev' is not implemented in v2 yet|command 'perf' is not implemented in v2 yet|command 'perfcmp' is not implemented in v2 yet|command 'matrix' is not implemented in v2 yet" "$ROOT/wrela-v2/src/application" --glob '*_command.wr' >/dev/null; then
  echo "v2 cli bootstrap violation: deferred not-implemented stubs detected in command modules" >&2
  exit 1
fi

if ! rg -n "test_update_command_rejects_path_argument|test_update_command_returns_two_for_missing_manifest|test_update_command_returns_zero_for_up_to_date_manifest|test_update_command_returns_zero_for_newer_artifact_and_writes_state" "$ROOT/wrela-v2/tests/contract/update_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: update command contract scenarios missing" >&2
  exit 1
fi

if ! rg -n "test_verify_cert_command_requires_path|test_verify_cert_command_returns_one_for_missing_file|test_verify_cert_command_returns_two_for_missing_required_field|test_verify_cert_command_returns_zero_for_valid_cert" "$ROOT/wrela-v2/tests/contract/verify_cert_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: verify-cert command contract scenarios missing" >&2
  exit 1
fi
if ! rg -n "test_verify_cert_command_returns_two_for_hash_mismatch" "$ROOT/wrela-v2/tests/contract/verify_cert_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: verify-cert hash mismatch contract scenario missing" >&2
  exit 1
fi
if ! rg -n "test_build_command_cert_output_is_deterministic_for_repeated_builds" "$ROOT/wrela-v2/tests/contract/build_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: deterministic cert output contract scenario missing" >&2
  exit 1
fi

if ! rg -n "test_dev_command_requires_path|test_dev_command_completes_bounded_cycles_for_valid_project|test_dev_command_returns_two_when_build_or_run_fails" "$ROOT/wrela-v2/tests/contract/dev_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: dev command contract scenarios missing" >&2
  exit 1
fi

if ! rg -n "test_perf_command_default_path_returns_two_for_invalid_workspace|test_perf_command_explicit_valid_path_writes_report|test_perf_command_returns_two_when_any_run_fails" "$ROOT/wrela-v2/tests/contract/perf_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perf command contract scenarios missing" >&2
  exit 1
fi
if ! rg -n "test_perf_command_report_is_deterministic_for_repeated_runs|test_perf_command_percentiles_are_deterministic_for_odd_even_sample_counts" "$ROOT/wrela-v2/tests/contract/perf_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: deterministic perf contract scenarios missing" >&2
  exit 1
fi

if ! rg -n "test_perfcmp_command_returns_one_when_baseline_missing|test_perfcmp_command_returns_two_when_regression_exceeds_threshold|test_perfcmp_command_returns_zero_within_threshold" "$ROOT/wrela-v2/tests/contract/perfcmp_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perfcmp command contract scenarios missing" >&2
  exit 1
fi
if ! rg -n "test_perfcmp_command_returns_one_for_invalid_baseline_schema" "$ROOT/wrela-v2/tests/contract/perfcmp_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: perfcmp invalid baseline schema scenario missing" >&2
  exit 1
fi

if ! rg -n "test_matrix_command_executes_fixed_cells_for_valid_path|test_matrix_command_returns_two_when_any_cell_fails" "$ROOT/wrela-v2/tests/contract/matrix_command_contract_test.wr" >/dev/null; then
  echo "v2 cli bootstrap violation: matrix command contract scenarios missing" >&2
  exit 1
fi

if ! rg -n "test_cli_routes_full_m7_surface_and_core_path_validation" "$CONTRACT" >/dev/null; then
  echo "v2 cli bootstrap violation: required CLI contract scenario missing in $CONTRACT" >&2
  exit 1
fi

if rg -n "mock_success|skip_[a-z_]+|placeholder_impl|fake_success" "$ROOT/wrela-v2/src/application" --glob '*_command.wr' >/dev/null; then
  echo "v2 cli bootstrap violation: bypass/fake-success markers detected in v2 command modules" >&2
  rg -n "mock_success|skip_[a-z_]+|placeholder_impl|fake_success" "$ROOT/wrela-v2/src/application" --glob '*_command.wr' >&2 || true
  exit 1
fi

echo "v2 cli bootstrap check passed"
