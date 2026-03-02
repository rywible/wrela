#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/jupiter_integration_blocking_matrix.sh --sha <commit-sha> [--skip-fly]

Runs the Jupiter full-send blocking matrix:
  1) Runtime lib correctness
  2) Core cluster correctness pack
  3) Strict local perf evidence gates
  4) Fly drills (default-on; use --skip-fly for local debug)
USAGE
}

require_tool() {
  local tool="$1"
  if ! command -v "$tool" >/dev/null 2>&1; then
    echo "error: missing required tool: $tool" >&2
    exit 1
  fi
}

require_exec() {
  local path="$1"
  [[ -x "$path" ]] || {
    echo "error: missing executable: $path" >&2
    exit 1
  }
}

SHA_INPUT=""
WITH_FLY="1"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      SHA_INPUT="${2:-}"
      shift 2
      ;;
    --skip-fly)
      WITH_FLY="0"
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "error: unknown argument: $1" >&2
      usage
      exit 2
      ;;
  esac
done

[[ -n "$SHA_INPUT" ]] || {
  usage
  exit 2
}

require_tool cargo
require_tool git
require_tool jq

SHA="$(git -C "$ROOT" rev-parse --verify "${SHA_INPUT}^{commit}" 2>/dev/null || true)"
[[ -n "$SHA" ]] || {
  echo "error: invalid commit SHA: $SHA_INPUT" >&2
  exit 1
}
[[ "${#SHA}" -eq 40 ]] || {
  echo "error: resolved SHA is not full-length: $SHA" >&2
  exit 1
}

HEAD_SHA="$(git -C "$ROOT" rev-parse HEAD)"
[[ "$HEAD_SHA" == "$SHA" ]] || {
  echo "error: HEAD ($HEAD_SHA) must match pinned SHA ($SHA)" >&2
  exit 1
}

require_exec "$ROOT/scripts/perf/local_db_write_baseline_capture.sh"
require_exec "$ROOT/scripts/perf/local_db_write_meso_compare.sh"
require_exec "$ROOT/scripts/perf/local_db_write_profile_sweep.sh"

if [[ "$WITH_FLY" == "1" ]]; then
  require_exec "$ROOT/scripts/fly/wrela_deploy_smoke.sh"
  require_exec "$ROOT/scripts/fly/wrela_write_load_test.sh"
  require_exec "$ROOT/scripts/fly/wreladb_cluster_drill.sh"
  require_exec "$ROOT/scripts/fly/wreladb_chaos_loop.sh"
fi

echo "==> [1/4] runtime lib correctness"
(
  cd "$ROOT"
  cargo test -p wrela_runtime --lib
)

echo "==> [2/4] core cluster correctness pack"
(
  cd "$ROOT"
  cargo test -p wrela_runtime \
    --test db_consensus_faults \
    --test db_wal_failure_isolation \
    --test db_local_cluster_smoke \
    --test db_local_cluster_rolling \
    --test db_local_cluster_runtime_stability
)

echo "==> [3/4] strict local perf evidence"
(
  cd "$ROOT"
  scripts/perf/local_db_write_baseline_capture.sh --sha "$SHA"
  scripts/perf/local_db_write_meso_compare.sh
  scripts/perf/local_db_write_profile_sweep.sh
)

if [[ "$WITH_FLY" == "1" ]]; then
  echo "==> [4/4] fly drills"
  (
    cd "$ROOT"
    scripts/fly/wrela_deploy_smoke.sh
    scripts/fly/wrela_write_load_test.sh
    scripts/fly/wreladb_cluster_drill.sh
    scripts/fly/wreladb_chaos_loop.sh
  )
else
  echo "==> [4/4] fly drills skipped (--skip-fly; local debug only, not a final gate)"
fi

echo "blocking matrix complete for SHA: $SHA (with_fly=$WITH_FLY)"
