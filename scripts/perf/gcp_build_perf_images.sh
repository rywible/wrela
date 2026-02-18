#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--arches "amd64 arm64"] [--suffix <name>]

Builds prewarmed custom images and updates image families.

Environment:
  GCP_PROJECT                        (default: gcloud configured project)
  GCP_ZONE                           (default: us-central1-a)
  GCP_IMAGE_PROJECT                  (default: GCP_PROJECT)
  GCP_AMD64_BUILDER_MACHINE_TYPE     (default: n2-standard-4)
  GCP_ARM64_BUILDER_MACHINE_TYPE     (default: t2a-standard-4)
  GCP_AMD64_SOURCE_IMAGE_FAMILY      (default: ubuntu-2404-lts-amd64)
  GCP_ARM64_SOURCE_IMAGE_FAMILY      (default: ubuntu-2404-lts-arm64)
  GCP_AMD64_IMAGE_FAMILY             (default: wrela-perf-amd64)
  GCP_ARM64_IMAGE_FAMILY             (default: wrela-perf-arm64)
  DISK_SIZE_GB                       (default: 120)
  WARM_WRELA                         (default: 1)
  WARM_SYNC_MODE                     (default: archive, one of archive|git)
  KEEP_BUILDERS                      (default: 0)
USAGE
}

ARCHES="amd64 arm64"
SUFFIX=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --arches)
      ARCHES="${2:-}"
      shift 2
      ;;
    --suffix)
      SUFFIX="${2:-}"
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
IMAGE_PROJECT="${GCP_IMAGE_PROJECT:-${PROJECT}}"
DISK_SIZE_GB="${DISK_SIZE_GB:-120}"
WARM_WRELA="${WARM_WRELA:-1}"
WARM_SYNC_MODE="${WARM_SYNC_MODE:-archive}"
KEEP_BUILDERS="${KEEP_BUILDERS:-0}"

GCP_AMD64_BUILDER_MACHINE_TYPE="${GCP_AMD64_BUILDER_MACHINE_TYPE:-n2-standard-4}"
GCP_ARM64_BUILDER_MACHINE_TYPE="${GCP_ARM64_BUILDER_MACHINE_TYPE:-t2a-standard-4}"
GCP_AMD64_SOURCE_IMAGE_FAMILY="${GCP_AMD64_SOURCE_IMAGE_FAMILY:-ubuntu-2404-lts-amd64}"
GCP_ARM64_SOURCE_IMAGE_FAMILY="${GCP_ARM64_SOURCE_IMAGE_FAMILY:-ubuntu-2404-lts-arm64}"
GCP_AMD64_IMAGE_FAMILY="${GCP_AMD64_IMAGE_FAMILY:-wrela-perf-amd64}"
GCP_ARM64_IMAGE_FAMILY="${GCP_ARM64_IMAGE_FAMILY:-wrela-perf-arm64}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

if [[ "${WARM_SYNC_MODE}" != "archive" && "${WARM_SYNC_MODE}" != "git" ]]; then
  echo "WARM_SYNC_MODE must be archive or git" >&2
  exit 1
fi

stamp="$(date +%Y%m%d-%H%M%S)"
if [[ -z "${SUFFIX}" ]]; then
  SUFFIX="${stamp}"
fi

sanitize_name() {
  echo "$1" | tr '[:upper:]' '[:lower:]' | tr -cs 'a-z0-9-' '-' | sed 's/^-*//; s/-*$//' | cut -c1-63
}

delete_instance_if_exists() {
  local name="$1"
  gcloud compute instances delete "${name}" --project "${PROJECT}" --zone "${ZONE}" --quiet >/dev/null 2>&1 || true
}

run_ssh_with_retry() {
  local instance="$1"
  local command="$2"
  local attempts=0
  local max=6
  local sleep_s=10
  while (( attempts < max )); do
    attempts=$((attempts + 1))
    if gcloud compute ssh "${instance}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command "${command}"; then
      return 0
    fi
    if (( attempts < max )); then
      echo "[${instance}] ssh attempt ${attempts}/${max} failed; retrying in ${sleep_s}s" >&2
      sleep "${sleep_s}"
    fi
  done
  return 1
}

run_scp_with_retry() {
  local src="$1"
  local dst="$2"
  local attempts=0
  local max=4
  local sleep_s=8
  while (( attempts < max )); do
    attempts=$((attempts + 1))
    if gcloud compute scp --project "${PROJECT}" --zone "${ZONE}" "${src}" "${dst}" --quiet; then
      return 0
    fi
    if (( attempts < max )); then
      echo "[scp] attempt ${attempts}/${max} failed; retrying in ${sleep_s}s" >&2
      sleep "${sleep_s}"
    fi
  done
  return 1
}

run_remote_bootstrap() {
  local instance="$1"
  run_ssh_with_retry "${instance}" '
set -euo pipefail
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
if ! command -v cargo >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
source "$HOME/.cargo/env"
rustup toolchain install stable >/dev/null
rustup default stable >/dev/null
'
}

sync_workspace_to_builder() {
  local instance="$1"
  local remote_url
  remote_url="$(git -C "${ROOT}" remote get-url origin 2>/dev/null || true)"
  local branch
  branch="$(git -C "${ROOT}" rev-parse --abbrev-ref HEAD)"

if [[ "${WARM_SYNC_MODE}" == "git" && -n "${remote_url}" ]]; then
    run_ssh_with_retry "${instance}" "
set -euo pipefail
rm -rf \"\$HOME/wrela\"
git clone --depth=1 --branch \"${branch}\" \"${remote_url}\" \"\$HOME/wrela\"
"
    return 0
  fi

  local tarball
  tarball="$(mktemp /tmp/wrela-image-seed-XXXXXX.tar.gz)"
  (cd "${ROOT}" && COPYFILE_DISABLE=1 tar --exclude .git --exclude target --exclude .artifacts --exclude '.DS_Store' --exclude '._*' -czf "${tarball}" .)
  run_scp_with_retry "${tarball}" "${instance}:~/wrela-image-seed.tar.gz"
  rm -f "${tarball}"

  run_ssh_with_retry "${instance}" '
set -euo pipefail
rm -rf "$HOME/wrela"
mkdir -p "$HOME/wrela"
tar -xzf "$HOME/wrela-image-seed.tar.gz" -C "$HOME/wrela"
find "$HOME/wrela" -name "._*" -type f -delete
'
}

warm_wrela_build() {
  local instance="$1"
  run_ssh_with_retry "${instance}" '
set -euo pipefail
source "$HOME/.cargo/env"
cd "$HOME/wrela"
cargo fetch
cargo build -p wrela --release
'
}

build_arch_image() {
  local arch="$1"
  local machine_type="$2"
  local source_image_family="$3"
  local target_family="$4"

  local builder
  builder="$(sanitize_name "wrela-image-builder-${arch}-${SUFFIX}")"
  local image_name
  image_name="$(sanitize_name "${target_family}-${SUFFIX}")"

  delete_instance_if_exists "${builder}"

  gcloud compute instances create "${builder}" \
    --project "${PROJECT}" \
    --zone "${ZONE}" \
    --machine-type "${machine_type}" \
    --image-project ubuntu-os-cloud \
    --image-family "${source_image_family}" \
    --boot-disk-size "${DISK_SIZE_GB}GB" \
    --labels "owner=codex,purpose=wrela-perf-image,arch=${arch}"

  run_remote_bootstrap "${builder}"

  if [[ "${WARM_WRELA}" == "1" ]]; then
    sync_workspace_to_builder "${builder}"
    warm_wrela_build "${builder}"
  fi

  gcloud compute instances stop "${builder}" --project "${PROJECT}" --zone "${ZONE}" --quiet

  gcloud compute images create "${image_name}" \
    --project "${IMAGE_PROJECT}" \
    --source-disk "${builder}" \
    --source-disk-zone "${ZONE}" \
    --family "${target_family}" \
    --description "Wrela perf prewarmed image (${arch}) built ${stamp}"

  if [[ "${KEEP_BUILDERS}" != "1" ]]; then
    delete_instance_if_exists "${builder}"
  fi

  echo "Built image: ${IMAGE_PROJECT}/${image_name} (family=${target_family}, arch=${arch})"
}

for arch in ${ARCHES}; do
  case "${arch}" in
    amd64)
      build_arch_image "amd64" "${GCP_AMD64_BUILDER_MACHINE_TYPE}" "${GCP_AMD64_SOURCE_IMAGE_FAMILY}" "${GCP_AMD64_IMAGE_FAMILY}"
      ;;
    arm64)
      build_arch_image "arm64" "${GCP_ARM64_BUILDER_MACHINE_TYPE}" "${GCP_ARM64_SOURCE_IMAGE_FAMILY}" "${GCP_ARM64_IMAGE_FAMILY}"
      ;;
    *)
      echo "Unknown arch: ${arch}" >&2
      exit 1
      ;;
  esac
done

echo "Done. Latest family images now available:"
echo "  amd64 family: ${GCP_AMD64_IMAGE_FAMILY}"
echo "  arm64 family: ${GCP_ARM64_IMAGE_FAMILY}"
