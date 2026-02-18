#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 --run-id <id> [--arches "amd64 arm64"] [--out-env <path>]

Environment:
  GCP_PROJECT                        (default: gcloud configured project)
  GCP_IMAGE_PROJECT                  (default: GCP_PROJECT)
  GCP_ZONE                           (default: us-central1-a)
  NAME_PREFIX                        (default: wrela-perf-ephem)
  GCP_AMD64_MACHINE_TYPE             (default: n2-standard-4)
  GCP_ARM64_MACHINE_TYPE             (default: t2a-standard-4)
  GCP_AMD64_IMAGE_FAMILY             (default: wrela-perf-amd64)
  GCP_ARM64_IMAGE_FAMILY             (default: wrela-perf-arm64)
  DISK_SIZE_GB                       (default: 150)
  GCP_USE_SPOT                       (default: 1)
  GCP_SPOT_MAX_RETRIES               (default: 3)
  GCP_SPOT_BACKOFF_SEC               (default: 20)
  GCP_ALLOW_FALLBACK_ONDEMAND        (default: 0)
USAGE
}

RUN_ID=""
ARCHES="amd64 arm64"
OUT_ENV=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --run-id)
      RUN_ID="${2:-}"
      shift 2
      ;;
    --arches)
      ARCHES="${2:-}"
      shift 2
      ;;
    --out-env)
      OUT_ENV="${2:-}"
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

if [[ -z "${RUN_ID}" ]]; then
  echo "--run-id is required" >&2
  usage
  exit 1
fi

PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null)}"
IMAGE_PROJECT="${GCP_IMAGE_PROJECT:-${PROJECT}}"
ZONE="${GCP_ZONE:-us-central1-a}"
NAME_PREFIX="${NAME_PREFIX:-wrela-perf-ephem}"
DISK_SIZE_GB="${DISK_SIZE_GB:-150}"
GCP_USE_SPOT="${GCP_USE_SPOT:-1}"
GCP_SPOT_MAX_RETRIES="${GCP_SPOT_MAX_RETRIES:-3}"
GCP_SPOT_BACKOFF_SEC="${GCP_SPOT_BACKOFF_SEC:-20}"
GCP_ALLOW_FALLBACK_ONDEMAND="${GCP_ALLOW_FALLBACK_ONDEMAND:-0}"

GCP_AMD64_MACHINE_TYPE="${GCP_AMD64_MACHINE_TYPE:-n2-standard-4}"
GCP_ARM64_MACHINE_TYPE="${GCP_ARM64_MACHINE_TYPE:-t2a-standard-4}"
GCP_AMD64_IMAGE_FAMILY="${GCP_AMD64_IMAGE_FAMILY:-wrela-perf-amd64}"
GCP_ARM64_IMAGE_FAMILY="${GCP_ARM64_IMAGE_FAMILY:-wrela-perf-arm64}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

if [[ -z "${OUT_ENV}" ]]; then
  OUT_ENV="$(mktemp)"
fi

LABEL_RUN_ID="$(echo "${RUN_ID}" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9_-' '-' | cut -c1-63)"

sanitize_name() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9-' '-' | sed 's/^-*//; s/-*$//' | cut -c1-63
}

is_preempt_or_capacity_error() {
  local log_path="$1"
  rg -qi "preempt|preemption|spot|insufficient|resource.*unavailable|capacity|ZONE_RESOURCE_POOL_EXHAUSTED" "${log_path}"
}

is_quota_error() {
  local log_path="$1"
  rg -qi "CPUS_ALL_REGIONS|quota.*exceeded|Limit:" "${log_path}"
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

delete_if_exists() {
  local name="$1"
  gcloud compute instances delete "${name}" --project "${PROJECT}" --zone "${ZONE}" --quiet >/dev/null 2>&1 || true
}

create_single() {
  local arch="$1"
  local machine_type="$2"
  local image_family="$3"
  local instance_name="$4"

  local attempts=0
  local mode="ondemand"
  local create_reason=""
  local log_file
  log_file="$(mktemp)"

  local use_spot="${GCP_USE_SPOT}"
  if [[ "${use_spot}" == "1" ]]; then
    mode="spot"
  fi

  local max_try
  max_try="${GCP_SPOT_MAX_RETRIES}"
  if [[ "${use_spot}" != "1" ]]; then
    max_try=1
  fi

  local success=0

  for ((i=1; i<=max_try; i++)); do
    attempts=$i
    delete_if_exists "${instance_name}"

    common_flags=(
      --project "${PROJECT}"
      --zone "${ZONE}"
      --machine-type "${machine_type}"
      --image-project "${IMAGE_PROJECT}"
      --image-family "${image_family}"
      --boot-disk-size "${DISK_SIZE_GB}GB"
      --labels "owner=codex,purpose=wrela-perf,run_id=${LABEL_RUN_ID},arch=${arch}"
    )

    if [[ "${use_spot}" == "1" ]]; then
      common_flags+=(--provisioning-model=SPOT --instance-termination-action=STOP)
    fi

    if gcloud compute instances create "${instance_name}" "${common_flags[@]}" >"${log_file}" 2>&1; then
      success=1
      create_reason="created"
      break
    fi

    if is_quota_error "${log_file}"; then
      create_reason="infra_quota"
    elif is_preempt_or_capacity_error "${log_file}"; then
      create_reason="infra_preempted"
    else
      create_reason="infra_error"
    fi

    if [[ $i -lt $max_try ]]; then
      sleep_secs="$(compute_backoff "$i" "${GCP_SPOT_BACKOFF_SEC}")"
      echo "[${arch}] create attempt ${i}/${max_try} failed (${create_reason}); retrying in ${sleep_secs}s" >&2
      sleep "${sleep_secs}"
    fi
  done

  if [[ $success -ne 1 && "${use_spot}" == "1" && "${GCP_ALLOW_FALLBACK_ONDEMAND}" == "1" ]]; then
    echo "[${arch}] spot failed after ${max_try} attempts; trying on-demand fallback" >&2
    attempts=$((attempts + 1))
    mode="ondemand_fallback"
    delete_if_exists "${instance_name}"
    fallback_flags=(
      --project "${PROJECT}"
      --zone "${ZONE}"
      --machine-type "${machine_type}"
      --image-project "${IMAGE_PROJECT}"
      --image-family "${image_family}"
      --boot-disk-size "${DISK_SIZE_GB}GB"
      --labels "owner=codex,purpose=wrela-perf,run_id=${LABEL_RUN_ID},arch=${arch},fallback=ondemand"
    )
    if gcloud compute instances create "${instance_name}" "${fallback_flags[@]}" >"${log_file}" 2>&1; then
      success=1
      create_reason="created"
    else
      if is_quota_error "${log_file}"; then
        create_reason="infra_quota"
      elif is_preempt_or_capacity_error "${log_file}"; then
        create_reason="infra_preempted"
      else
        create_reason="infra_error"
      fi
    fi
  fi

  if [[ $success -ne 1 ]]; then
    echo "[${arch}] failed to create instance ${instance_name} after ${attempts} attempts (reason=${create_reason})" >&2
    cat "${log_file}" >&2
    rm -f "${log_file}"
    return 1
  fi

  rm -f "${log_file}"

  upper_arch="$(echo "${arch}" | tr '[:lower:]' '[:upper:]')"
  {
    echo "${upper_arch}_INSTANCE=${instance_name}"
    echo "${upper_arch}_CREATE_ATTEMPTS=${attempts}"
    echo "${upper_arch}_CREATE_MODE=${mode}"
    echo "${upper_arch}_CREATE_REASON=${create_reason}"
  } >> "${OUT_ENV}"
}

: > "${OUT_ENV}"

echo "PROJECT=${PROJECT}" >> "${OUT_ENV}"
echo "IMAGE_PROJECT=${IMAGE_PROJECT}" >> "${OUT_ENV}"
echo "ZONE=${ZONE}" >> "${OUT_ENV}"
echo "RUN_ID=${RUN_ID}" >> "${OUT_ENV}"

safe_run_id="$(sanitize_name "${RUN_ID}")"

for arch in ${ARCHES}; do
  case "${arch}" in
    amd64)
      name="$(sanitize_name "${NAME_PREFIX}-${safe_run_id}-amd64")"
      create_single "amd64" "${GCP_AMD64_MACHINE_TYPE}" "${GCP_AMD64_IMAGE_FAMILY}" "${name}"
      ;;
    arm64)
      name="$(sanitize_name "${NAME_PREFIX}-${safe_run_id}-arm64")"
      create_single "arm64" "${GCP_ARM64_MACHINE_TYPE}" "${GCP_ARM64_IMAGE_FAMILY}" "${name}"
      ;;
    *)
      echo "Unknown arch in --arches: ${arch}" >&2
      exit 1
      ;;
  esac
done

echo "WROTE_ENV=${OUT_ENV}"
