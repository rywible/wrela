#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--sha <commit>] [--run-id <id>]

Environment:
  PERF_SUITES             (default: micro meso macro linux)
  PERF_RUNS               (default: 5)
  FLY_POOL_CONFIG         (default: scripts/perf/fly_pool.json)
  FLY_LOCK_ROOT           (default: ~/.codex/state/wrela-perf-fly-locks)
  FLY_CLAIM_TIMEOUT_SEC   (default: 600)
  FLY_POLL_INTERVAL_SEC   (default: 10)
  FLY_START_TIMEOUT_SEC   (default: 180)
  KEEP_FAILED_MACHINES    (default: 0)
USAGE
}

SHA=""
RUN_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha) SHA="${2:-}"; shift 2 ;;
    --run-id) RUN_ID="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PERF_SUITES="${PERF_SUITES:-micro meso macro linux}"
PERF_RUNS="${PERF_RUNS:-5}"
POOL_CONFIG="${FLY_POOL_CONFIG:-${ROOT}/scripts/perf/fly_pool.json}"
LOCK_ROOT="${FLY_LOCK_ROOT:-${HOME}/.codex/state/wrela-perf-fly-locks}"
CLAIM_TIMEOUT_SEC="${FLY_CLAIM_TIMEOUT_SEC:-600}"
POLL_INTERVAL_SEC="${FLY_POLL_INTERVAL_SEC:-10}"
START_TIMEOUT_SEC="${FLY_START_TIMEOUT_SEC:-180}"
KEEP_FAILED_MACHINES="${KEEP_FAILED_MACHINES:-0}"

mkdir -p "${LOCK_ROOT}"

if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi
if ! command -v flock >/dev/null 2>&1; then
  echo "error: flock is required" >&2
  exit 1
fi
if ! command -v flyctl >/dev/null 2>&1; then
  echo "error: flyctl is required" >&2
  exit 1
fi

"${ROOT}/scripts/perf/fly_pool_validate.sh" "${POOL_CONFIG}" >/dev/null

if [[ -z "${SHA}" ]]; then
  SHA="$(git -C "${ROOT}" rev-parse HEAD)"
fi

if ! git -C "${ROOT}" fetch -q origin "${SHA}" >/dev/null 2>&1; then
  echo "error: sha ${SHA} not found on origin. push first." >&2
  exit 1
fi

short_sha="$(echo "${SHA}" | cut -c1-12)"
if [[ -z "${RUN_ID}" ]]; then
  RUN_ID="pr-$(date +%Y%m%d-%H%M%S)-${short_sha}"
fi

OUT_ROOT="${ROOT}/.artifacts/perf/fly/${RUN_ID}"
mkdir -p "${OUT_ROOT}/amd64"

CLAIMED_NAME=""
CLAIMED_APP=""
CLAIMED_MACHINE_ID=""
CLAIMED_REGION=""
CLAIMED_LOCK_FILE=""
CLAIMED_LOCK_META=""
CLAIMED_LOCK_FD=""

release_claim() {
  if [[ -n "${CLAIMED_LOCK_META}" ]]; then
    rm -f "${CLAIMED_LOCK_META}" || true
  fi
  if [[ -n "${CLAIMED_LOCK_FD}" ]]; then
    flock -u "${CLAIMED_LOCK_FD}" >/dev/null 2>&1 || true
    eval "exec ${CLAIMED_LOCK_FD}>&-" || true
  fi
}

machine_stop_best_effort() {
  if [[ -n "${CLAIMED_APP}" && -n "${CLAIMED_MACHINE_ID}" ]]; then
    flyctl machine stop "${CLAIMED_MACHINE_ID}" -a "${CLAIMED_APP}" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  if [[ "${KEEP_FAILED_MACHINES}" != "1" ]]; then
    machine_stop_best_effort
  fi
  release_claim
}
trap cleanup EXIT

claim_machine() {
  local start_ts now candidate lock_file lock_meta fd rec
  start_ts="$(date +%s)"

  while true; do
    while IFS= read -r rec; do
      candidate_name="$(jq -r '.name' <<<"${rec}")"
      candidate_app="$(jq -r '.app' <<<"${rec}")"
      candidate_machine="$(jq -r '.machine_id' <<<"${rec}")"
      candidate_region="$(jq -r '.region' <<<"${rec}")"

      lock_file="${LOCK_ROOT}/${candidate_app}-${candidate_machine}.lock"
      lock_meta="${lock_file}.meta.json"

      eval "exec {fd}>\"${lock_file}\""
      if flock -n "${fd}"; then
        CLAIMED_NAME="${candidate_name}"
        CLAIMED_APP="${candidate_app}"
        CLAIMED_MACHINE_ID="${candidate_machine}"
        CLAIMED_REGION="${candidate_region}"
        CLAIMED_LOCK_FILE="${lock_file}"
        CLAIMED_LOCK_META="${lock_meta}"
        CLAIMED_LOCK_FD="${fd}"

        cat > "${CLAIMED_LOCK_META}" <<JSON
{
  "pid": $$,
  "host": "$(hostname)",
  "worktree": "${ROOT}",
  "run_id": "${RUN_ID}",
  "claimed_at_unix_ms": $(( $(date +%s) * 1000 ))
}
JSON
        return 0
      fi
      eval "exec ${fd}>&-"
    done < <(jq -c '.runners[] | select(.enabled == true)' "${POOL_CONFIG}")

    now="$(date +%s)"
    if (( now - start_ts >= CLAIM_TIMEOUT_SEC )); then
      return 1
    fi
    sleep "${POLL_INTERVAL_SEC}"
  done
}

wait_machine_started() {
  local start_ts now state
  start_ts="$(date +%s)"
  while true; do
    state="$(flyctl machine list -a "${CLAIMED_APP}" --json 2>/dev/null | jq -r --arg id "${CLAIMED_MACHINE_ID}" '.[] | select(.id == $id) | (.state // .status // .instance_state // "")' | head -n1 || true)"
    if [[ "${state}" == "started" ]]; then
      return 0
    fi
    now="$(date +%s)"
    if (( now - start_ts >= START_TIMEOUT_SEC )); then
      echo "machine failed to reach started state: ${CLAIMED_APP}/${CLAIMED_MACHINE_ID} (state=${state})" >&2
      return 1
    fi
    sleep 2
  done
}

if ! claim_machine; then
  echo "error: unable to claim fly perf machine within timeout (${CLAIM_TIMEOUT_SEC}s)" >&2
  exit 1
fi

run_log="${OUT_ROOT}/amd64/run.log"
if ! flyctl machine start "${CLAIMED_MACHINE_ID}" -a "${CLAIMED_APP}" >"${OUT_ROOT}/amd64/start.log" 2>&1; then
  echo "error: failed to start machine ${CLAIMED_APP}/${CLAIMED_MACHINE_ID}" >&2
  exit 1
fi
if ! wait_machine_started >"${OUT_ROOT}/amd64/state.log" 2>&1; then
  exit 1
fi

final_status="failed"
final_reason="infra_error"
set +e
PERF_SUITES="${PERF_SUITES}" PERF_RUNS="${PERF_RUNS}" \
  "${ROOT}/scripts/perf/fly_sync_branch_and_run.sh" \
  --app "${CLAIMED_APP}" --machine "${CLAIMED_MACHINE_ID}" --sha "${SHA}" \
  --out-dir "${OUT_ROOT}/amd64/artifacts" >"${run_log}" 2>&1
run_rc=$?
set -e

if [[ ${run_rc} -eq 0 ]]; then
  final_status="passed"
  final_reason="ok"
else
  if rg -qi "wireguard|connection timed out|connection closed|No route to host|network is unreachable|machine.*not found|proxy error|ssh" "${run_log}"; then
    final_reason="infra_unavailable"
  elif rg -qi "perf gate failed|error: failed to copy perf artifacts|failed to reach started" "${run_log}"; then
    final_reason="infra_error"
  else
    final_reason="perf_failed"
  fi
fi

generated_at_unix_ms="$(( $(date +%s) * 1000 ))"
SUMMARY_PATH="${OUT_ROOT}/summary.json"
cat > "${SUMMARY_PATH}" <<JSON
{
  "version": 2,
  "provider": "fly",
  "run_id": "${RUN_ID}",
  "sha": "${SHA}",
  "generated_at_unix_ms": ${generated_at_unix_ms},
  "pool_config": "${POOL_CONFIG}",
  "overall": {
    "status": "${final_status}",
    "reason": "${final_reason}"
  },
  "arch": {
    "amd64": {
      "status": "${final_status}",
      "reason": "${final_reason}",
      "attempts": 1,
      "runner": {
        "name": "${CLAIMED_NAME}",
        "app": "${CLAIMED_APP}",
        "machine_id": "${CLAIMED_MACHINE_ID}",
        "region": "${CLAIMED_REGION}"
      }
    }
  }
}
JSON

echo "Perf summary: ${SUMMARY_PATH}"

if [[ "${final_status}" == "passed" ]]; then
  exit 0
fi
exit 1
