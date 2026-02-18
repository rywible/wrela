#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--sha <commit>] [--run-id <id>]

Environment:
  GCP_PROJECT                        (default: gcloud configured project)
  GCP_ZONE                           (default: us-central1-a)
  PERF_SUITES                        (default: micro meso macro linux)
  PERF_RUNS                          (default: 3)
  SYNC_MODE                          (default: archive)
  GCP_SPOT_MAX_RETRIES               (default: 3)
  GCP_SPOT_BACKOFF_SEC               (default: 20)
  GCP_USE_SPOT                       (default: 1)
  GCP_ALLOW_FALLBACK_ONDEMAND        (default: 0)
  KEEP_FAILED_INSTANCES              (default: 0)
USAGE
}

SHA=""
RUN_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      SHA="${2:-}"
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
      echo "Unknown arg: $1" >&2
      usage
      exit 1
      ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null)}"
ZONE="${GCP_ZONE:-us-central1-a}"
PERF_SUITES="${PERF_SUITES:-micro meso macro linux}"
PERF_RUNS="${PERF_RUNS:-3}"
SYNC_MODE="${SYNC_MODE:-archive}"
GCP_SPOT_MAX_RETRIES="${GCP_SPOT_MAX_RETRIES:-3}"
GCP_SPOT_BACKOFF_SEC="${GCP_SPOT_BACKOFF_SEC:-20}"
GCP_USE_SPOT="${GCP_USE_SPOT:-1}"
GCP_ALLOW_FALLBACK_ONDEMAND="${GCP_ALLOW_FALLBACK_ONDEMAND:-0}"
KEEP_FAILED_INSTANCES="${KEEP_FAILED_INSTANCES:-0}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

if [[ -z "${SHA}" ]]; then
  SHA="$(git -C "${ROOT}" rev-parse HEAD)"
fi

short_sha="$(echo "${SHA}" | cut -c1-12)"
if [[ -z "${RUN_ID}" ]]; then
  RUN_ID="pr-$(date +%Y%m%d-%H%M%S)-${short_sha}"
fi
BASE_RUN_ID="${RUN_ID}"

OUT_ROOT="${ROOT}/.artifacts/perf/gcp/${RUN_ID}"
mkdir -p "${OUT_ROOT}"

is_preempt_or_capacity_error() {
  local log_path="$1"
  rg -qi "preempt|preemption|spot|insufficient|resource.*unavailable|capacity|ZONE_RESOURCE_POOL_EXHAUSTED|terminated" "${log_path}"
}

is_quota_error() {
  local log_path="$1"
  rg -qi "CPUS_ALL_REGIONS|quota.*exceeded|Limit:" "${log_path}"
}

is_infra_transport_error() {
  local log_path="$1"
  rg -qi "Permission denied \\(publickey\\)|gcloud\\.compute\\.(ssh|scp)|connection timed out|connection closed|network is unreachable|No route to host|broken pipe|ssh_exchange_identification|failed to copy perf artifacts" "${log_path}"
}

compute_backoff() {
  local attempt="$1"
  local base="$2"
  local jitter=0
  if [[ -n "${RANDOM:-}" ]]; then
    jitter=$((RANDOM % 7))
  fi
  echo $((base * attempt + jitter))
}

delete_instance() {
  local name="$1"
  if [[ -z "${name}" ]]; then
    return 0
  fi
  gcloud compute instances delete "${name}" --project "${PROJECT}" --zone "${ZONE}" --quiet >/dev/null 2>&1 || true
}

amd64_status="not_run"
amd64_reason=""
amd64_attempts=0
amd64_create_attempts_total=0
amd64_instance=""

arm64_status="not_run"
arm64_reason=""
arm64_attempts=0
arm64_create_attempts_total=0
arm64_instance=""

overall_status="failed"
overall_reason="infra_error"

run_arch() {
  local arch="$1"
  local arch_out_dir="${OUT_ROOT}/${arch}"
  mkdir -p "${arch_out_dir}"

  local final_status="failed"
  local final_reason="infra_error"
  local run_attempts=0
  local create_attempts_total=0
  local last_instance=""

  for ((attempt=1; attempt<=GCP_SPOT_MAX_RETRIES; attempt++)); do
    run_attempts=$attempt

    local create_env
    create_env="$(mktemp)"
    local create_log="${arch_out_dir}/create-attempt-${attempt}.log"

    local attempt_run_id="${BASE_RUN_ID}-${arch}-a${attempt}"
    if OUT_ENV="${create_env}" RUN_ID="${attempt_run_id}" ARCHES="${arch}" \
      GCP_PROJECT="${PROJECT}" GCP_ZONE="${ZONE}" GCP_USE_SPOT="${GCP_USE_SPOT}" \
      GCP_SPOT_MAX_RETRIES="${GCP_SPOT_MAX_RETRIES}" GCP_SPOT_BACKOFF_SEC="${GCP_SPOT_BACKOFF_SEC}" \
      GCP_ALLOW_FALLBACK_ONDEMAND="${GCP_ALLOW_FALLBACK_ONDEMAND}" \
      "${ROOT}/scripts/perf/gcp_create_ephemeral_instances.sh" --run-id "${attempt_run_id}" --arches "${arch}" --out-env "${create_env}" >"${create_log}" 2>&1; then
      # shellcheck disable=SC1090
      source "${create_env}"
      RUN_ID="${BASE_RUN_ID}"
    else
      if is_quota_error "${create_log}"; then
        final_reason="infra_quota"
      elif is_preempt_or_capacity_error "${create_log}"; then
        final_reason="infra_preempted"
      else
        final_reason="infra_error"
      fi
      rm -f "${create_env}"
      if [[ $attempt -lt $GCP_SPOT_MAX_RETRIES ]]; then
        sleep_secs="$(compute_backoff "${attempt}" "${GCP_SPOT_BACKOFF_SEC}")"
        echo "[${arch}] create failed (${final_reason}); retrying in ${sleep_secs}s" >&2
        sleep "${sleep_secs}"
        continue
      fi
      break
    fi

    local upper
    upper="$(echo "${arch}" | tr '[:lower:]' '[:upper:]')"
    local instance_var="${upper}_INSTANCE"
    local create_attempts_var="${upper}_CREATE_ATTEMPTS"
    local instance_name="${!instance_var:-}"
    local create_attempts="${!create_attempts_var:-1}"
    create_attempts_total=$((create_attempts_total + create_attempts))
    last_instance="${instance_name}"

    local run_log="${arch_out_dir}/run-attempt-${attempt}.log"
    if GCP_PROJECT="${PROJECT}" GCP_ZONE="${ZONE}" START_INSTANCE=0 STOP_WHEN_DONE=1 STOP_MODE=delete \
      SYNC_MODE="${SYNC_MODE}" TARGET_REF="${SHA}" PERF_SUITES="${PERF_SUITES}" PERF_RUNS="${PERF_RUNS}" \
      "${ROOT}/scripts/perf/gcp_sync_branch_and_run.sh" "${instance_name}" "${ZONE}" >"${run_log}" 2>&1; then
      artifacts_path="$(sed -n 's/^Artifacts: //p' "${run_log}" | tail -n 1)"
      if [[ -n "${artifacts_path}" && -d "${artifacts_path}" ]]; then
        rm -rf "${arch_out_dir}/artifacts"
        mkdir -p "${arch_out_dir}/artifacts"
        cp -R "${artifacts_path}/." "${arch_out_dir}/artifacts/"
      fi
      final_status="passed"
      final_reason="ok"
      rm -f "${create_env}"
      break
    fi

    if is_preempt_or_capacity_error "${run_log}"; then
      final_reason="infra_preempted"
      delete_instance "${instance_name}"
      rm -f "${create_env}"
      if [[ $attempt -lt $GCP_SPOT_MAX_RETRIES ]]; then
        sleep_secs="$(compute_backoff "${attempt}" "${GCP_SPOT_BACKOFF_SEC}")"
        echo "[${arch}] run preempted/capacity issue; retrying in ${sleep_secs}s" >&2
        sleep "${sleep_secs}"
        continue
      fi
      break
    elif is_infra_transport_error "${run_log}"; then
      final_reason="infra_error"
      delete_instance "${instance_name}"
      rm -f "${create_env}"
      if [[ $attempt -lt $GCP_SPOT_MAX_RETRIES ]]; then
        sleep_secs="$(compute_backoff "${attempt}" "${GCP_SPOT_BACKOFF_SEC}")"
        echo "[${arch}] infra transport error; retrying in ${sleep_secs}s" >&2
        sleep "${sleep_secs}"
        continue
      fi
      break
    else
      final_reason="perf_failed"
      if [[ "${KEEP_FAILED_INSTANCES}" != "1" ]]; then
        delete_instance "${instance_name}"
      fi
      rm -f "${create_env}"
      break
    fi
  done

  case "${arch}" in
    amd64)
      amd64_status="${final_status}"
      amd64_reason="${final_reason}"
      amd64_attempts="${run_attempts}"
      amd64_create_attempts_total="${create_attempts_total}"
      amd64_instance="${last_instance}"
      ;;
    arm64)
      arm64_status="${final_status}"
      arm64_reason="${final_reason}"
      arm64_attempts="${run_attempts}"
      arm64_create_attempts_total="${create_attempts_total}"
      arm64_instance="${last_instance}"
      ;;
  esac

  if [[ "${final_status}" == "passed" ]]; then
    return 0
  fi
  return 1
}

run_arch amd64 || true
run_arch arm64 || true

if [[ "${amd64_status}" == "passed" && "${arm64_status}" == "passed" ]]; then
  overall_status="passed"
  overall_reason="ok"
else
  if [[ "${amd64_reason}" == "infra_quota" || "${arm64_reason}" == "infra_quota" ]]; then
    overall_reason="infra_quota"
  elif [[ "${amd64_reason}" == "infra_preempted" || "${arm64_reason}" == "infra_preempted" ]]; then
    overall_reason="infra_preempted"
  elif [[ "${amd64_reason}" == "perf_failed" || "${arm64_reason}" == "perf_failed" ]]; then
    overall_reason="perf_failed"
  else
    overall_reason="infra_error"
  fi
fi

generated_at_unix_ms="$(( $(date +%s) * 1000 ))"
SUMMARY_PATH="${OUT_ROOT}/summary.json"
cat > "${SUMMARY_PATH}" <<JSON
{
  "version": 1,
  "run_id": "${RUN_ID}",
  "sha": "${SHA}",
  "project": "${PROJECT}",
  "zone": "${ZONE}",
  "generated_at_unix_ms": ${generated_at_unix_ms},
  "spot": {
    "enabled": ${GCP_USE_SPOT},
    "max_retries": ${GCP_SPOT_MAX_RETRIES},
    "backoff_sec": ${GCP_SPOT_BACKOFF_SEC},
    "allow_fallback_ondemand": ${GCP_ALLOW_FALLBACK_ONDEMAND}
  },
  "overall": {
    "status": "${overall_status}",
    "reason": "${overall_reason}"
  },
  "arch": {
    "amd64": {
      "status": "${amd64_status}",
      "reason": "${amd64_reason}",
      "attempts": ${amd64_attempts},
      "create_attempts_total": ${amd64_create_attempts_total},
      "last_instance": "${amd64_instance}"
    },
    "arm64": {
      "status": "${arm64_status}",
      "reason": "${arm64_reason}",
      "attempts": ${arm64_attempts},
      "create_attempts_total": ${arm64_create_attempts_total},
      "last_instance": "${arm64_instance}"
    }
  }
}
JSON

echo "Perf summary: ${SUMMARY_PATH}"

if [[ "${overall_status}" == "passed" ]]; then
  exit 0
fi

exit 1
