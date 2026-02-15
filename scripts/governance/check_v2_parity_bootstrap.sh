#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITE="$ROOT/wrela-v2/tools/parity/contract_suite.wr"
RUNNER="$ROOT/wrela-v2/tools/parity/runner.wr"
APP_PARITY="$ROOT/wrela-v2/src/application/parity_command.wr"
APP_CLI="$ROOT/wrela-v2/src/application/cli.wr"
CONTRACT="$ROOT/wrela-v2/tests/contract/parity_command_contract_test.wr"

if [[ ! -f "$SUITE" ]]; then
  echo "missing v2 parity suite: $SUITE" >&2
  exit 1
fi

if [[ ! -f "$RUNNER" ]]; then
  echo "missing v2 parity runner: $RUNNER" >&2
  exit 1
fi

if [[ ! -f "$APP_PARITY" ]]; then
  echo "missing app parity command: $APP_PARITY" >&2
  exit 1
fi

if [[ ! -f "$APP_CLI" ]]; then
  echo "missing app cli module: $APP_CLI" >&2
  exit 1
fi

if [[ ! -f "$CONTRACT" ]]; then
  echo "missing parity contract file: $CONTRACT" >&2
  exit 1
fi

required_suite_symbols=(
  "scenario_help_surface"
  "scenario_parse_error_exit_code"
  "scenario_type_error_exit_code"
  "scenario_test_list_ledger_lite"
  "scenario_cert_schema_fixture_fields"
)

for symbol in "${required_suite_symbols[@]}"; do
  if ! rg -n "$symbol" "$SUITE" >/dev/null; then
    echo "v2 parity bootstrap violation: missing scenario '$symbol' in $SUITE" >&2
    exit 1
  fi
done

if ! rg -n "try_to_run_v1_command|try_to_run_host_command|get_exit_code|get_stdout" "$RUNNER" >/dev/null; then
  echo "v2 parity bootstrap violation: missing required runner helpers in $RUNNER" >&2
  exit 1
fi

if ! rg -n "run_parity_command|run_contract_suite|status=pass|status=fail" "$APP_PARITY" >/dev/null; then
  echo "v2 parity bootstrap violation: missing required parity command wiring/reporting in $APP_PARITY" >&2
  exit 1
fi

if ! rg -n "command == \"parity\"|run_parity_command" "$APP_CLI" >/dev/null; then
  echo "v2 parity bootstrap violation: cli parity dispatch missing in $APP_CLI" >&2
  exit 1
fi

if ! rg -n "test_parity_exit_code_defaults_to_one|test_parity_stdout_text_defaults_to_empty_string|test_parity_command_requires_environment_when_unset" "$CONTRACT" >/dev/null; then
  echo "v2 parity bootstrap violation: required contract scenarios missing in $CONTRACT" >&2
  exit 1
fi

echo "v2 parity bootstrap check passed"
