#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ASSERT_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_schema.sh"
STRICT_ASSERT_SCRIPT="$ROOT/scripts/perf/assert_strict_local_db_write_evidence.sh"
INDEX_SCRIPT="$ROOT/scripts/perf/index_local_db_write_artifact.sh"
CLAIM_SCRIPT="$ROOT/scripts/perf/assert_local_db_write_claimable.sh"
ARTIFACT_ROOT="$ROOT/.artifacts/perf/local-db-write"
BASELINES_DIR="$ARTIFACT_ROOT/baselines"
CANONICAL_PATH="$ARTIFACT_ROOT/CANONICAL.json"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/local_db_write_baseline_capture.sh --sha <commit-sha> [--no-promote] [--allow-dirty]

Runs the local DB write perf harness under anti-cheat defaults, validates schema,
and captures baseline metadata.
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool: $tool" >&2
    exit 1
  fi
}

fail() {
  echo "error: $*" >&2
  exit 1
}

is_uint() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

SHA_INPUT=""
PROMOTE="1"
ALLOW_DIRTY="0"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      SHA_INPUT="${2:-}"
      shift 2
      ;;
    --no-promote)
      PROMOTE="0"
      shift
      ;;
    --allow-dirty)
      ALLOW_DIRTY="1"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      fail "unknown argument: $1"
      ;;
  esac
done

[[ -n "$SHA_INPUT" ]] || {
  usage
  exit 2
}

require_tool git
require_tool jq
require_tool cargo
[[ -x "$ASSERT_SCRIPT" ]] || fail "missing schema assert script: $ASSERT_SCRIPT"
[[ -x "$STRICT_ASSERT_SCRIPT" ]] || fail "missing strict evidence assert script: $STRICT_ASSERT_SCRIPT"
[[ -x "$INDEX_SCRIPT" ]] || fail "missing artifact index script: $INDEX_SCRIPT"
[[ -x "$CLAIM_SCRIPT" ]] || fail "missing claimability script: $CLAIM_SCRIPT"

SHA="$(git -C "$ROOT" rev-parse --verify "${SHA_INPUT}^{commit}" 2>/dev/null || true)"
[[ -n "$SHA" ]] || fail "invalid commit SHA: $SHA_INPUT"
[[ "${#SHA}" -eq 40 ]] || fail "resolved SHA is not full-length: $SHA"

if [[ "$ALLOW_DIRTY" != "1" ]]; then
  if ! git -C "$ROOT" fetch -q origin "$SHA"; then
    fail "SHA is not fetchable from origin: $SHA"
  fi
  if ! git -C "$ROOT" branch -r --contains "$SHA" | grep -q 'origin/'; then
    fail "SHA is not contained in any origin/* ref: $SHA"
  fi
fi

HEAD_SHA="$(git -C "$ROOT" rev-parse HEAD)"
[[ "$HEAD_SHA" == "$SHA" ]] || fail "HEAD ($HEAD_SHA) must match pinned SHA ($SHA)"
if [[ "$ALLOW_DIRTY" != "1" ]]; then
  if ! git -C "$ROOT" diff --quiet --ignore-submodules --; then
    fail "working tree has unstaged changes; baseline capture requires clean tree (or pass --allow-dirty)"
  fi
  if ! git -C "$ROOT" diff --cached --quiet --ignore-submodules --; then
    fail "working tree has staged changes; baseline capture requires clean tree (or pass --allow-dirty)"
  fi
fi

PERF_DURATION="${WRELA_LOCAL_PERF_DURATION_SECONDS:-8}"
PERF_CONCURRENCY="${WRELA_LOCAL_PERF_CONCURRENCY:-24}"
PERF_PAYLOAD="${WRELA_LOCAL_PERF_PAYLOAD_BYTES:-128}"
REQUIRE_REAL_QUORUM="1"
REQUIRE_LANE_SPREAD="${WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD:-}"

if [[ -z "$REQUIRE_LANE_SPREAD" ]]; then
  # Writer lane count is now a typed DbConfig field (default 1 in test mode).
  REQUIRE_LANE_SPREAD="0"
fi

is_uint "$PERF_DURATION" || fail "WRELA_LOCAL_PERF_DURATION_SECONDS must be an integer"
is_uint "$PERF_CONCURRENCY" || fail "WRELA_LOCAL_PERF_CONCURRENCY must be an integer"
is_uint "$PERF_PAYLOAD" || fail "WRELA_LOCAL_PERF_PAYLOAD_BYTES must be an integer"
[[ "$REQUIRE_LANE_SPREAD" == "0" || "$REQUIRE_LANE_SPREAD" == "1" ]] || fail "WRELA_LOCAL_PERF_REQUIRE_LANE_SPREAD must be 0 or 1"

(( PERF_DURATION >= 8 )) || fail "duration too low for baseline evidence (need >= 8s)"
(( PERF_CONCURRENCY >= 24 )) || fail "concurrency too low for baseline evidence (need >= 24)"
(( PERF_PAYLOAD > 0 )) || fail "payload bytes must be > 0"

mkdir -p "$BASELINES_DIR"
mkdir -p "$ARTIFACT_ROOT"

before_latest="$(ls -1 "$ARTIFACT_ROOT" 2>/dev/null | grep -E '^[0-9]+$' | sort -n | tail -n 1 || true)"

(
  cd "$ROOT"
  WRELA_LOCAL_PERF_DURATION_SECONDS="$PERF_DURATION" \
  WRELA_LOCAL_PERF_CONCURRENCY="$PERF_CONCURRENCY" \
  WRELA_LOCAL_PERF_PAYLOAD_BYTES="$PERF_PAYLOAD" \
  cargo test -p wrela_runtime --release --test db_write_local_perf -- --ignored --nocapture
)

after_latest="$(ls -1 "$ARTIFACT_ROOT" 2>/dev/null | grep -E '^[0-9]+$' | sort -n | tail -n 1 || true)"
[[ -n "$after_latest" ]] || fail "no local-db-write run directory found after harness run"
if [[ -n "$before_latest" ]] && (( after_latest <= before_latest )); then
  fail "did not observe a newly created run directory (before=$before_latest after=$after_latest)"
fi

RUN_DIR="$ARTIFACT_ROOT/$after_latest"
SUMMARY_PATH="$RUN_DIR/summary.json"
[[ -f "$SUMMARY_PATH" ]] || fail "missing summary artifact: $SUMMARY_PATH"

"$ASSERT_SCRIPT" "$RUN_DIR"
"$STRICT_ASSERT_SCRIPT" "$RUN_DIR"
"$INDEX_SCRIPT" "$RUN_DIR" \
  --kind "baseline_capture" \
  --sha "$SHA" \
  --strict-required "$REQUIRE_REAL_QUORUM"
"$CLAIM_SCRIPT" "$RUN_DIR" \
  --strict-required "$REQUIRE_REAL_QUORUM" \
  --require-indexed 1 \
  --require-lane-spread "$REQUIRE_LANE_SPREAD"

short_sha="${SHA:0:12}"
baseline_path="$BASELINES_DIR/local-db-write-${short_sha}-${after_latest}.json"
now_ms="$(( $(date +%s) * 1000 ))"

jq -n \
  --arg sha "$SHA" \
  --arg run_id "$after_latest" \
  --arg summary_path "$SUMMARY_PATH" \
  --arg artifacts_root "$RUN_DIR" \
  --argjson generated_at_unix_ms "$now_ms" \
  --argjson min_duration_seconds 8 \
  --argjson min_concurrency 24 \
  --argjson require_real_quorum "$REQUIRE_REAL_QUORUM" \
  --argjson require_lane_spread "$REQUIRE_LANE_SPREAD" \
  --slurpfile summary "$SUMMARY_PATH" \
  '{
    version: 1,
    provider: "local-db-write",
    sha: $sha,
    status: "passed",
    generated_at_unix_ms: $generated_at_unix_ms,
    run_id: $run_id,
    artifacts_root: $artifacts_root,
    summary_path: $summary_path,
    schema_version: $summary[0].schema_version,
    config: $summary[0].config,
    anti_cheat: {
      min_duration_seconds: $min_duration_seconds,
      min_concurrency: $min_concurrency,
      require_real_quorum: ($require_real_quorum == 1),
      require_lane_spread: ($require_lane_spread == 1),
      no_dirty_tree: true,
      pinned_sha_required: true
    },
    workloads: [
      $summary[0].workloads[] | {
        name,
        attempts,
        success,
        failures,
        tps,
        p99_ms,
        p999_ms,
        replication_queue_depth: .replication.queue_depth,
        client_response_wait_pct: .client_write_path.response_wait_pct
      }
    ]
  }' >"$baseline_path"

if [[ "$PROMOTE" == "1" ]]; then
  jq -n \
    --arg sha "$SHA" \
    --arg run_id "$after_latest" \
    --arg status "passed" \
    --arg summary_path "$SUMMARY_PATH" \
    --arg baseline_path "$baseline_path" \
    --arg artifacts_root "$RUN_DIR" \
    --argjson generated_at_unix_ms "$now_ms" \
    --slurpfile summary "$SUMMARY_PATH" \
    '{
      version: 1,
      sha: $sha,
      status: $status,
      generated_at_unix_ms: $generated_at_unix_ms,
      run_id: $run_id,
      artifacts_root: $artifacts_root,
      summary_path: $summary_path,
      baseline_path: $baseline_path,
      schema_version: $summary[0].schema_version,
      config: $summary[0].config,
      profile: "local-db-write-default"
    }' >"$CANONICAL_PATH"
fi

echo "Local baseline summary: $SUMMARY_PATH"
echo "Local baseline capture: $baseline_path"
if [[ "$PROMOTE" == "1" ]]; then
  echo "Local canonical pointer: $CANONICAL_PATH"
fi
