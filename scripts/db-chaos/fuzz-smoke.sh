#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FUZZ_DIR="$REPO_ROOT/runtime/fuzz"
ARTIFACT_DIR="${ARTIFACT_DIR:-$REPO_ROOT/artifacts/fuzz}"
TOOLCHAIN="${FUZZ_TOOLCHAIN:-nightly}"
RUNS="${FUZZ_RUNS:-512}"
MAX_LEN="${FUZZ_MAX_LEN:-4096}"
TIMEOUT_SECS="${FUZZ_TIMEOUT_SECS:-5}"

mkdir -p "$ARTIFACT_DIR"

if ! cargo fuzz --help >/dev/null 2>&1; then
  echo "cargo-fuzz is not installed; install with: cargo install cargo-fuzz"
  exit 0
fi

targets=(
  sql_parse_statement
  grpc_write_batch_validate
  transport_chaos_classifier
  wal_decode_path
  multipart_session_decode
  schema_job_store_decode
)

for target in "${targets[@]}"; do
  echo "[fuzz-smoke] target=$target runs=$RUNS max_len=$MAX_LEN timeout=${TIMEOUT_SECS}s"
  (
    cd "$FUZZ_DIR"
    cargo +"$TOOLCHAIN" fuzz run "$target" "corpus/$target" -- \
      -runs="$RUNS" \
      -max_len="$MAX_LEN" \
      -timeout="$TIMEOUT_SECS" \
      -artifact_prefix="$ARTIFACT_DIR/$target-"
  )
done

echo "fuzz smoke completed; artifacts prefixed at $ARTIFACT_DIR"
