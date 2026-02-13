#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT_DIR="$ROOT/.artifacts/baselines/v1"
mkdir -p "$OUT_DIR"

pushd "$ROOT" >/dev/null

run_capture() {
  local out="$1"
  shift
  set +e
  "$@" > "$out" 2>&1
  local status=$?
  set -e
  echo "$status"
}

HELP_STATUS=$(run_capture "$OUT_DIR/help.txt" cargo run -q -p wrela -- --help)
LEDGER_STATUS=$(run_capture "$OUT_DIR/ledger_lite_test.txt" cargo run -q -p wrela -- test apps/ledger-lite)
SPEC_STATUS=$(run_capture "$OUT_DIR/language_spec_check.txt" cargo run -q -p wrela -- check language/spec/spec.wr)
MICRO_STATUS=$(run_capture "$OUT_DIR/micro_perf_smoke.txt" cargo run -q -p wrela -- perf benchmarks/micro --profile=smoke --runs=1)
MESO_STATUS=$(run_capture "$OUT_DIR/meso_perf_smoke.txt" cargo run -q -p wrela -- perf benchmarks/meso --profile=smoke --runs=1)

{
  echo "captured_at_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "git_head=$(git rev-parse --verify HEAD 2>/dev/null || true)"
  echo "rustc=$(rustc --version 2>/dev/null || true)"
  echo "cargo=$(cargo --version 2>/dev/null || true)"
  echo "status_help=$HELP_STATUS"
  echo "status_ledger_lite=$LEDGER_STATUS"
  echo "status_language_spec_check=$SPEC_STATUS"
  echo "status_micro_perf_smoke=$MICRO_STATUS"
  echo "status_meso_perf_smoke=$MESO_STATUS"
} > "$OUT_DIR/manifest.txt"

popd >/dev/null

echo "wrote baseline artifacts to $OUT_DIR"
