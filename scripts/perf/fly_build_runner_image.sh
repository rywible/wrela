#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--image <ref>] [--tag <tag>] [--toolchain <toolchain>]

Environment:
  FLY_PERF_IMAGE_REPO   (default: registry.fly.io/wrela-perf-runner)
  IMAGE_TAG             (default: <date>-<git-short-sha>)
  RUST_TOOLCHAIN        (default: stable)
  DOCKER_PLATFORM       (default: linux/amd64)
USAGE
}

IMAGE_REPO="${FLY_PERF_IMAGE_REPO:-registry.fly.io/wrela-perf-runner}"
IMAGE_TAG="${IMAGE_TAG:-}"
RUST_TOOLCHAIN="${RUST_TOOLCHAIN:-stable}"
DOCKER_PLATFORM="${DOCKER_PLATFORM:-linux/amd64}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --image) IMAGE_REPO="${2:-}"; shift 2 ;;
    --tag) IMAGE_TAG="${2:-}"; shift 2 ;;
    --toolchain) RUST_TOOLCHAIN="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [[ -z "${IMAGE_TAG}" ]]; then
  IMAGE_TAG="$(date +%Y%m%d)-$(git -C "${ROOT}" rev-parse --short HEAD)"
fi

IMAGE_REF="${IMAGE_REPO}:${IMAGE_TAG}"

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker is required" >&2
  exit 1
fi
if ! command -v flyctl >/dev/null 2>&1; then
  echo "error: flyctl is required" >&2
  exit 1
fi

echo "Authenticating docker to Fly registry..."
flyctl auth docker >/dev/null

if ! docker buildx version >/dev/null 2>&1; then
  echo "error: docker buildx is required" >&2
  exit 1
fi

echo "Building and pushing image: ${IMAGE_REF} (${DOCKER_PLATFORM})"
docker buildx build \
  --platform "${DOCKER_PLATFORM}" \
  -f "${ROOT}/scripts/perf/fly/Dockerfile" \
  --build-arg "RUST_TOOLCHAIN=${RUST_TOOLCHAIN}" \
  -t "${IMAGE_REF}" \
  --push \
  "${ROOT}"

echo "Fly perf image ready: ${IMAGE_REF}"
