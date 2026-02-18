#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--count <n>] [--name-prefix <prefix>] [--region <region>] [--vm-size <size>] --image <ref> [--org <org>] [--pool-out <path>] [--refresh]

Creates or updates one-machine-per-app Fly perf runners and writes fly pool config.

Environment:
  FLY_POOL_COUNT        (default: 6)
  FLY_POOL_PREFIX       (default: wrela-perf-runner)
  FLY_REGION            (default: iad)
  FLY_VM_SIZE           (default: performance-4x)
  FLY_ORG               (optional)
  FLY_POOL_CONFIG       (default: scripts/perf/fly_pool.json)
USAGE
}

COUNT="${FLY_POOL_COUNT:-6}"
PREFIX="${FLY_POOL_PREFIX:-wrela-perf-runner}"
REGION="${FLY_REGION:-iad}"
VM_SIZE="${FLY_VM_SIZE:-performance-4x}"
ORG="${FLY_ORG:-}"
IMAGE_REF="${FLY_PERF_IMAGE:-}"
POOL_OUT=""
REFRESH=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --count) COUNT="${2:-}"; shift 2 ;;
    --name-prefix) PREFIX="${2:-}"; shift 2 ;;
    --region) REGION="${2:-}"; shift 2 ;;
    --vm-size) VM_SIZE="${2:-}"; shift 2 ;;
    --image) IMAGE_REF="${2:-}"; shift 2 ;;
    --org) ORG="${2:-}"; shift 2 ;;
    --pool-out) POOL_OUT="${2:-}"; shift 2 ;;
    --refresh) REFRESH=1; shift 1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -z "${POOL_OUT}" ]]; then
  POOL_OUT="${FLY_POOL_CONFIG:-${ROOT}/scripts/perf/fly_pool.json}"
fi

if [[ -z "${IMAGE_REF}" ]]; then
  echo "error: --image is required (example: registry.fly.io/wrela-perf-runner:20260218-abc123)" >&2
  exit 1
fi
if ! command -v flyctl >/dev/null 2>&1; then
  echo "error: flyctl is required" >&2
  exit 1
fi
if ! command -v jq >/dev/null 2>&1; then
  echo "error: jq is required" >&2
  exit 1
fi

ensure_app() {
  local app="$1"
  if flyctl status -a "${app}" >/dev/null 2>&1; then
    return 0
  fi
  if [[ -n "${ORG}" ]]; then
    flyctl apps create "${app}" --org "${ORG}" --yes >/dev/null
  else
    flyctl apps create "${app}" --yes >/dev/null
  fi
}

get_machine_id() {
  local app="$1"
  flyctl machine list -a "${app}" --json | jq -r '.[0].id // empty'
}

create_machine() {
  local app="$1"
  local machine_name="runner"
  flyctl machine run "${IMAGE_REF}" \
    -a "${app}" \
    --region "${REGION}" \
    --vm-size "${VM_SIZE}" \
    --vm-cpu-kind performance \
    --name "${machine_name}" \
    --restart always \
    --detach \
    --command "sleep infinity" >/dev/null
}

update_machine() {
  local app="$1"
  local machine_id="$2"
  flyctl machine update "${machine_id}" \
    -a "${app}" \
    --image "${IMAGE_REF}" \
    --vm-size "${VM_SIZE}" \
    --vm-cpu-kind performance \
    --yes >/dev/null
}

stop_machine() {
  local app="$1"
  local machine_id="$2"
  flyctl machine stop "${machine_id}" -a "${app}" >/dev/null 2>&1 || true
}

runners_json='[]'

for i in $(seq 1 "${COUNT}"); do
  app="${PREFIX}-${i}"
  echo "Ensuring app: ${app}"
  ensure_app "${app}"

  machine_id="$(get_machine_id "${app}")"
  if [[ -z "${machine_id}" ]]; then
    echo "Creating machine for ${app}"
    create_machine "${app}"
    sleep 2
    machine_id="$(get_machine_id "${app}")"
    if [[ -z "${machine_id}" ]]; then
      echo "error: failed to discover created machine for ${app}" >&2
      exit 1
    fi
  elif [[ "${REFRESH}" == "1" ]]; then
    echo "Refreshing machine ${machine_id} for ${app}"
    update_machine "${app}" "${machine_id}"
  fi

  stop_machine "${app}" "${machine_id}"

  runner_json="$(jq -n \
    --arg name "${app}" \
    --arg app "${app}" \
    --arg machine_id "${machine_id}" \
    --arg region "${REGION}" \
    '{name:$name, app:$app, machine_id:$machine_id, region:$region, cpu_kind:"performance", enabled:true}')"
  runners_json="$(jq -c --argjson item "${runner_json}" '. + [$item]' <<<"${runners_json}")"
done

mkdir -p "$(dirname "${POOL_OUT}")"
jq -n --argjson runners "${runners_json}" '{version:1, runners:$runners}' > "${POOL_OUT}"

echo "Wrote pool config: ${POOL_OUT}"
