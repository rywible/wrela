#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
GATE_SCRIPT="$ROOT/scripts/perf/fly_pr_perf_gate.sh"
MAIN_ROOT="$ROOT/.artifacts/perf/main"
CANONICAL_PATH="$MAIN_ROOT/CANONICAL.json"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/fly_refresh_main_baseline.sh --sha <commit-sha> [--pool <path>] [--run-id <id>]

Runs the Fly perf gate for a pinned main SHA and, on pass, refreshes
.artifacts/perf/main/CANONICAL.json + snapshot artifacts.
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

SHA_INPUT=""
POOL_CONFIG="$ROOT/scripts/perf/fly_pool.json"
RUN_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      SHA_INPUT="${2:-}"
      shift 2
      ;;
    --pool)
      POOL_CONFIG="${2:-}"
      shift 2
      ;;
    --run-id)
      RUN_ID="${2:-}"
      shift 2
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
[[ -x "$GATE_SCRIPT" ]] || fail "missing executable gate script: $GATE_SCRIPT"
[[ -f "$POOL_CONFIG" ]] || fail "missing pool config: $POOL_CONFIG"

SHA="$(git -C "$ROOT" rev-parse --verify "${SHA_INPUT}^{commit}" 2>/dev/null || true)"
[[ -n "$SHA" ]] || fail "invalid commit SHA: $SHA_INPUT"
[[ "${#SHA}" -eq 40 ]] || fail "resolved SHA is not full-length: $SHA"

short_sha="${SHA:0:12}"
if [[ -z "$RUN_ID" ]]; then
  RUN_ID="main-refresh-$(date -u +%Y%m%d-%H%M%S)-${short_sha}"
fi

mkdir -p "$MAIN_ROOT"
summary_path="$ROOT/.artifacts/perf/fly/$RUN_ID/summary.json"
refresh_receipt_path="$MAIN_ROOT/refresh-${RUN_ID}.json"

if "$GATE_SCRIPT" --sha "$SHA" --pool "$POOL_CONFIG" --run-id "$RUN_ID"; then
  gate_rc=0
else
  gate_rc=$?
fi

overall_reason="$(jq -r '.overall.reason // "summary_missing"' "$summary_path" 2>/dev/null || echo "summary_missing")"

if [[ "$gate_rc" -eq 0 ]]; then
  state="passed"
else
  if [[ "$overall_reason" == "perf_failed" ]]; then
    state="perf_failed"
  else
    state="failed"
  fi
fi

now_ms="$(( $(date +%s) * 1000 ))"
jq -n \
  --argjson version 1 \
  --arg run_id "$RUN_ID" \
  --arg target_sha "$SHA" \
  --arg state "$state" \
  --argjson gate_rc "$gate_rc" \
  --arg overall_reason "$overall_reason" \
  --arg summary_path "$summary_path" \
  --arg canonical_pointer "$CANONICAL_PATH" \
  --argjson generated_at_unix_ms "$now_ms" \
  '{
    version: $version,
    run_id: $run_id,
    target_sha: $target_sha,
    state: $state,
    gate_rc: $gate_rc,
    overall_reason: $overall_reason,
    summary_path: $summary_path,
    canonical_pointer: $canonical_pointer,
    generated_at_unix_ms: $generated_at_unix_ms
  }' >"$refresh_receipt_path"

if [[ "$gate_rc" -ne 0 ]]; then
  echo "Perf summary: $summary_path"
  echo "Refresh receipt: $refresh_receipt_path"
  exit "$gate_rc"
fi

run_root="$ROOT/.artifacts/perf/fly/$RUN_ID"
arch_root="$run_root/amd64"
[[ -d "$arch_root" ]] || fail "missing run arch artifacts: $arch_root"
[[ -f "$summary_path" ]] || fail "missing run summary: $summary_path"

main_sha_root="$MAIN_ROOT/$SHA"
mkdir -p "$main_sha_root"
rm -rf "$main_sha_root/amd64"
cp -R "$arch_root" "$main_sha_root/amd64"
cp "$summary_path" "$main_sha_root/summary.json"

jq -n \
  --argjson version 2 \
  --arg sha "$SHA" \
  --arg status "passed" \
  --argjson generated_at_unix_ms "$now_ms" \
  --arg run_id "$RUN_ID" \
  --arg arch_scope "amd64_only" \
  --arg artifacts_root "$main_sha_root" \
  --arg summary_path "$main_sha_root/summary.json" \
  --arg suites "micro meso macro linux" \
  --arg profile "standard" \
  '{
    version: $version,
    sha: $sha,
    status: $status,
    generated_at_unix_ms: $generated_at_unix_ms,
    run_id: $run_id,
    arch_scope: $arch_scope,
    artifacts_root: $artifacts_root,
    summary_path: $summary_path,
    suites: $suites,
    profile: $profile
  }' >"$CANONICAL_PATH"

echo "Perf summary: $summary_path"
echo "Refresh receipt: $refresh_receipt_path"
echo "Main canonical pointer: $CANONICAL_PATH"
