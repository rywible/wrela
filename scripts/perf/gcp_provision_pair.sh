#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null)}"
ZONE="${GCP_ZONE:-us-central1-a}"
NAME_PREFIX="${NAME_PREFIX:-wrela-perf}"
X86_NAME="${X86_NAME:-${NAME_PREFIX}-x86-4c}"
ARM_NAME="${ARM_NAME:-${NAME_PREFIX}-arm-4c}"
X86_TYPE="${X86_TYPE:-n2-standard-4}"
ARM_TYPE="${ARM_TYPE:-t2a-standard-4}"
DISK_SIZE_GB="${DISK_SIZE_GB:-80}"
SPOT="${SPOT:-1}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

COMMON_FLAGS=(
  --project "${PROJECT}"
  --zone "${ZONE}"
  --boot-disk-size "${DISK_SIZE_GB}GB"
  --labels "owner=codex,purpose=wrela-perf"
)

if [[ "${SPOT}" == "1" ]]; then
  COMMON_FLAGS+=(--provisioning-model=SPOT --instance-termination-action=STOP)
fi

create_if_missing() {
  local name="$1"
  local machine_type="$2"
  local image_family="$3"

  if gcloud compute instances describe "${name}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null 2>&1; then
    echo "Instance already exists: ${name}"
    return 0
  fi

  echo "Creating ${name} (${machine_type}, ${image_family})"
  gcloud compute instances create "${name}" \
    "${COMMON_FLAGS[@]}" \
    --machine-type "${machine_type}" \
    --image-project ubuntu-os-cloud \
    --image-family "${image_family}"
}

bootstrap_instance() {
  local name="$1"
  echo "Bootstrapping ${name}"
  gcloud compute instances start "${name}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null
  gcloud compute ssh "${name}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command '
set -euo pipefail
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
if ! command -v cargo >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
'
}

create_if_missing "${X86_NAME}" "${X86_TYPE}" "ubuntu-2404-lts-amd64"
create_if_missing "${ARM_NAME}" "${ARM_TYPE}" "ubuntu-2404-lts-arm64"

bootstrap_instance "${X86_NAME}"
bootstrap_instance "${ARM_NAME}"

cat <<OUT
Provisioned:
- project: ${PROJECT}
- zone: ${ZONE}
- x86: ${X86_NAME} (${X86_TYPE})
- arm: ${ARM_NAME} (${ARM_TYPE})
- spot: ${SPOT}
OUT

cd "${ROOT}"
