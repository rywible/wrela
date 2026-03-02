#!/usr/bin/env bash
set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
POOL_CONFIG_DEFAULT="$ROOT/scripts/perf/fly_pool.json"
CANONICAL_PATH="$ROOT/.artifacts/perf/main/CANONICAL.json"
SUITES=(micro meso macro linux)

usage() {
  cat <<'USAGE'
Usage:
  scripts/perf/fly_pr_perf_gate.sh --sha <commit-sha> [--pool <path>] [--run-id <id>]

Runs pinned-SHA perf gate on a Fly runner, copies artifacts locally, and compares
candidate suite baselines against the canonical main baseline.
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

is_number() {
  [[ "$1" =~ ^[0-9]+([.][0-9]+)?$ ]]
}

SHA_INPUT=""
POOL_CONFIG="$POOL_CONFIG_DEFAULT"
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
require_tool flyctl
require_tool flock

[[ -f "$POOL_CONFIG" ]] || fail "missing pool config: $POOL_CONFIG"
[[ -f "$CANONICAL_PATH" ]] || fail "missing canonical baseline pointer: $CANONICAL_PATH"

PERF_RUNS="${PERF_RUNS:-10}"
PERF_WARMUP_RUNS="${PERF_WARMUP_RUNS:-1}"
PERF_CV_MAX_PCT="${PERF_CV_MAX_PCT:-10}"
MAX_THROUGHPUT_DROP_PCT="${WRELA_FLY_GATE_MAX_THROUGHPUT_DROP_PCT:-15}"
MAX_P99_REGRESSION_PCT="${WRELA_FLY_GATE_MAX_P99_REGRESSION_PCT:-10}"
INSTALL_DEPS_ON_VM="${WRELA_FLY_INSTALL_DEPS_ON_VM:-auto}"
FORCE_REBUILD_WRELA="${WRELA_FLY_FORCE_REBUILD_WRELA:-1}"

is_uint "$PERF_RUNS" || fail "PERF_RUNS must be an integer"
is_uint "$PERF_WARMUP_RUNS" || fail "PERF_WARMUP_RUNS must be an integer"
is_number "$PERF_CV_MAX_PCT" || fail "PERF_CV_MAX_PCT must be numeric"
is_number "$MAX_THROUGHPUT_DROP_PCT" || fail "WRELA_FLY_GATE_MAX_THROUGHPUT_DROP_PCT must be numeric"
is_number "$MAX_P99_REGRESSION_PCT" || fail "WRELA_FLY_GATE_MAX_P99_REGRESSION_PCT must be numeric"

(( PERF_RUNS >= 10 )) || fail "PERF_RUNS must be >= 10 for gate evidence"
(( PERF_WARMUP_RUNS >= 1 )) || fail "PERF_WARMUP_RUNS must be >= 1"

SHA="$(git -C "$ROOT" rev-parse --verify "${SHA_INPUT}^{commit}" 2>/dev/null || true)"
[[ -n "$SHA" ]] || fail "invalid commit SHA: $SHA_INPUT"
[[ "${#SHA}" -eq 40 ]] || fail "resolved SHA is not full-length: $SHA"

if ! git -C "$ROOT" fetch -q origin "$SHA"; then
  fail "SHA is not fetchable from origin: $SHA"
fi
if ! git -C "$ROOT" branch -r --contains "$SHA" | grep -q 'origin/'; then
  fail "SHA is not contained in any origin/* ref: $SHA"
fi

HEAD_SHA="$(git -C "$ROOT" rev-parse HEAD)"
[[ "$HEAD_SHA" == "$SHA" ]] || fail "HEAD ($HEAD_SHA) must match pinned SHA ($SHA)"
if ! git -C "$ROOT" diff --quiet --ignore-submodules --; then
  fail "working tree has unstaged changes; perf gate requires clean tree"
fi
if ! git -C "$ROOT" diff --cached --quiet --ignore-submodules --; then
  fail "working tree has staged changes; perf gate requires clean tree"
fi

APP="$(jq -er '.amd64.app | strings | select(length > 0)' "$POOL_CONFIG")"
MACHINE_ID="$(jq -er '.amd64.machine_id | strings | select(length > 0)' "$POOL_CONFIG")"
RUNNER_NAME="$(jq -er '.amd64.name | strings | select(length > 0)' "$POOL_CONFIG")"
RUNNER_REGION="$(jq -er '.amd64.region | strings | select(length > 0)' "$POOL_CONFIG")"

CANONICAL_ROOT="$(jq -er '.artifacts_root | strings | select(length > 0)' "$CANONICAL_PATH")"

for suite in "${SUITES[@]}"; do
  canonical_suite="$CANONICAL_ROOT/amd64/artifacts/perf/${suite}-baseline.json"
  [[ -f "$canonical_suite" ]] || fail "canonical suite baseline missing: $canonical_suite"
  if ! jq -e '
    (.summary.compile_throughput_tests_per_sec | type == "number" and . > 0)
    and (.summary.runtime_p99_ns | type == "number" and . > 0)
    and (.runs | type == "number" and . >= 1)
  ' "$canonical_suite" >/dev/null; then
    fail "canonical suite baseline schema invalid: $canonical_suite"
  fi
done

short_sha="${SHA:0:12}"
if [[ -z "$RUN_ID" ]]; then
  RUN_ID="pr-$(date -u +%Y%m%d-%H%M%S)-${short_sha}"
fi

RUN_ROOT="$ROOT/.artifacts/perf/fly/$RUN_ID"
ARCH_ROOT="$RUN_ROOT/amd64"
ARTIFACTS_DIR="$ARCH_ROOT/artifacts/perf"
SUMMARY_PATH="$RUN_ROOT/summary.json"
LOCK_PATH="$ROOT/.artifacts/perf/fly/.fly-perf.lock"

mkdir -p "$ROOT/.artifacts/perf/fly"
mkdir -p "$ARCH_ROOT"
mkdir -p "$ARTIFACTS_DIR"

exec 9>"$LOCK_PATH"
if ! flock -n 9; then
  fail "another Fly perf run is in progress (lock: $LOCK_PATH)"
fi

STATUS="failed"
REASON="not_started"
ATTEMPTS=0

write_summary() {
  local now_ms
  now_ms="$(( $(date +%s) * 1000 ))"

  jq -n \
    --argjson version 2 \
    --arg provider "fly" \
    --arg run_id "$RUN_ID" \
    --arg sha "$SHA" \
    --argjson generated_at_unix_ms "$now_ms" \
    --arg pool_config "$POOL_CONFIG" \
    --arg status "$STATUS" \
    --arg reason "$REASON" \
    --argjson attempts "$ATTEMPTS" \
    --arg runner_name "$RUNNER_NAME" \
    --arg app "$APP" \
    --arg machine_id "$MACHINE_ID" \
    --arg region "$RUNNER_REGION" \
    '{
      version: $version,
      provider: $provider,
      run_id: $run_id,
      sha: $sha,
      generated_at_unix_ms: $generated_at_unix_ms,
      pool_config: $pool_config,
      overall: {
        status: $status,
        reason: $reason
      },
      arch: {
        amd64: {
          status: $status,
          reason: $reason,
          attempts: $attempts,
          runner: {
            name: $runner_name,
            app: $app,
            machine_id: $machine_id,
            region: $region
          }
        }
      }
    }' >"$SUMMARY_PATH"
}

REMOTE_URL="$(git -C "$ROOT" remote get-url origin 2>/dev/null || true)"
[[ -n "$REMOTE_URL" ]] || fail "unable to read git origin URL"

suites_joined="${SUITES[*]}"
read -r -d '' REMOTE_CMD <<__REMOTE__ || true
set -Eeuo pipefail
if [[ -f "\$HOME/.cargo/env" ]]; then
  source "\$HOME/.cargo/env"
fi

mode="$INSTALL_DEPS_ON_VM"
need_install=0
if [[ "\${mode}" == "always" ]]; then
  need_install=1
elif [[ "\${mode}" == "auto" ]]; then
  for c in git jq clang; do
    if ! command -v "\$c" >/dev/null 2>&1; then
      need_install=1
      break
    fi
  done
  if [[ "\${need_install}" == "0" ]] && ! command -v cargo >/dev/null 2>&1; then
    need_install=1
  fi
fi

if [[ "\${need_install}" == "1" ]]; then
  apt_prefix=""
  if command -v sudo >/dev/null 2>&1; then
    apt_prefix="sudo"
  fi
  \${apt_prefix} apt-get update
  \${apt_prefix} DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
  if ! command -v cargo >/dev/null 2>&1; then
    curl https://sh.rustup.rs -sSf | sh -s -- -y
    source "\$HOME/.cargo/env"
  fi
fi

if [[ -d "\$HOME/wrela/.git" ]]; then
  cd "\$HOME/wrela"
  git remote set-url origin "$REMOTE_URL" || true
else
  rm -rf "\$HOME/wrela"
  git clone "$REMOTE_URL" "\$HOME/wrela"
  cd "\$HOME/wrela"
fi

git fetch origin "$SHA"
git checkout --detach FETCH_HEAD

pkill -f "target/release/wrela perf" >/dev/null 2>&1 || true
rm -rf .artifacts/perf
mkdir -p .artifacts/perf

if [[ ! -d "target/release" && -d "/opt/wrela-bootstrap/target/release" ]]; then
  mkdir -p target
  cp -R /opt/wrela-bootstrap/target/release target/ || true
fi

if [[ "$FORCE_REBUILD_WRELA" == "1" ]]; then
  cargo build -p wrela --release
fi

for suite in $suites_joined; do
  ./target/release/wrela perf --runs="$PERF_WARMUP_RUNS" --baseline-out=".artifacts/perf/\${suite}-warmup.json" "benchmarks/\${suite}"
  ./target/release/wrela perf --runs="$PERF_RUNS" --baseline-out=".artifacts/perf/\${suite}-baseline.json" "benchmarks/\${suite}"
done

uname -a > .artifacts/perf/host.txt
echo "Artifacts: \$HOME/wrela/.artifacts/perf"
__REMOTE__

escaped_remote_cmd="$(printf '%q' "$REMOTE_CMD")"

ATTEMPTS=$((ATTEMPTS + 1))
if ! flyctl machine start "$MACHINE_ID" -a "$APP" >"$ARCH_ROOT/start.log" 2>&1; then
  REASON="machine_start_failed"
  write_summary
  fail "failed to start machine ${APP}/${MACHINE_ID}"
fi

if ! flyctl ssh console -a "$APP" --machine "$MACHINE_ID" --command "bash -lc $escaped_remote_cmd" >"$ARCH_ROOT/run.log" 2>&1; then
  REASON="runner_exec_failed"
  write_summary
  fail "remote perf command failed on ${APP}/${MACHINE_ID}"
fi

flyctl machine status "$MACHINE_ID" -a "$APP" --json >"$ARCH_ROOT/state.log" 2>/dev/null || true

for suite in "${SUITES[@]}"; do
  for flavor in warmup baseline; do
    remote_name="${suite}-${flavor}.json"
    local_path="$ARTIFACTS_DIR/$remote_name"
    remote_cat_cmd="bash -lc 'cat \"\$HOME/wrela/.artifacts/perf/$remote_name\"'"
    if ! flyctl ssh console -a "$APP" --machine "$MACHINE_ID" --command "$remote_cat_cmd" >"$local_path" 2>/dev/null; then
      REASON="artifact_copy_failed"
      write_summary
      fail "failed to copy remote artifact: $remote_name"
    fi
  done
done

remote_cat_cmd="bash -lc 'cat \"\$HOME/wrela/.artifacts/perf/host.txt\"'"
if ! flyctl ssh console -a "$APP" --machine "$MACHINE_ID" --command "$remote_cat_cmd" >"$ARTIFACTS_DIR/host.txt" 2>/dev/null; then
  REASON="artifact_copy_failed"
  write_summary
  fail "failed to copy remote artifact: host.txt"
fi

perf_failed_suites=()

for suite in "${SUITES[@]}"; do
  cand_baseline="$ARTIFACTS_DIR/${suite}-baseline.json"
  cand_warmup="$ARTIFACTS_DIR/${suite}-warmup.json"
  base_baseline="$CANONICAL_ROOT/amd64/artifacts/perf/${suite}-baseline.json"
  comparison_path="$ARCH_ROOT/${suite}-comparison.json"

  if ! jq -e --argjson expected_runs "$PERF_RUNS" --argjson cv_max "$PERF_CV_MAX_PCT" '
    (.runs == $expected_runs)
    and (.summary.compile_throughput_tests_per_sec | type == "number" and . > 0)
    and (.summary.runtime_p99_ns | type == "number" and . > 0)
    and (.cv.compile_throughput_pct | type == "number" and . <= $cv_max)
    and (.cv.runtime_p50_pct | type == "number" and . <= $cv_max)
    and (.cv.runtime_p95_pct | type == "number" and . <= $cv_max)
    and (.cv.runtime_p99_pct | type == "number" and . <= $cv_max)
  ' "$cand_baseline" >/dev/null; then
    REASON="candidate_schema_invalid"
    write_summary
    fail "candidate baseline schema/CV invalid: $cand_baseline"
  fi

  if ! jq -e --argjson expected_warmup "$PERF_WARMUP_RUNS" '
    (.runs == $expected_warmup)
    and (.summary.compile_throughput_tests_per_sec | type == "number" and . > 0)
    and (.summary.runtime_p99_ns | type == "number" and . > 0)
  ' "$cand_warmup" >/dev/null; then
    REASON="candidate_schema_invalid"
    write_summary
    fail "candidate warmup schema invalid: $cand_warmup"
  fi

  jq -n \
    --arg suite "$suite" \
    --argfile base "$base_baseline" \
    --argfile cand "$cand_baseline" \
    '{
      suite: $suite,
      throughput: {
        baseline: $base.summary.compile_throughput_tests_per_sec,
        candidate: $cand.summary.compile_throughput_tests_per_sec,
        drop_pct: (
          if ($base.summary.compile_throughput_tests_per_sec | tonumber) <= 0 then null
          else ((($base.summary.compile_throughput_tests_per_sec - $cand.summary.compile_throughput_tests_per_sec) * 100) / $base.summary.compile_throughput_tests_per_sec)
          end
        )
      },
      p99: {
        baseline_ns: $base.summary.runtime_p99_ns,
        candidate_ns: $cand.summary.runtime_p99_ns,
        increase_pct: (
          if ($base.summary.runtime_p99_ns | tonumber) <= 0 then null
          else ((($cand.summary.runtime_p99_ns - $base.summary.runtime_p99_ns) * 100) / $base.summary.runtime_p99_ns)
          end
        )
      },
      candidate_runs: $cand.runs,
      candidate_cv: $cand.cv
    }' >"$comparison_path"

  if ! jq -e --argjson max_drop "$MAX_THROUGHPUT_DROP_PCT" --argjson max_p99 "$MAX_P99_REGRESSION_PCT" '
    (.throughput.drop_pct != null and .throughput.drop_pct <= $max_drop)
    and (.p99.increase_pct != null and .p99.increase_pct <= $max_p99)
  ' "$comparison_path" >/dev/null; then
    perf_failed_suites+=("$suite")
  fi
done

if (( ${#perf_failed_suites[@]} > 0 )); then
  REASON="perf_failed"
  STATUS="failed"
  write_summary
  echo "perf gate failed suites: ${perf_failed_suites[*]}" >&2
  exit 1
fi

STATUS="passed"
REASON="ok"
write_summary

echo "Perf summary: $SUMMARY_PATH"
