#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "Usage: $0 <instance-name> [zone]" >&2
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
INSTANCE="$1"
ZONE="${2:-${GCP_ZONE:-us-central1-a}}"
PROJECT="${GCP_PROJECT:-$(gcloud config get-value project 2>/dev/null)}"
BRANCH="${BRANCH:-$(git -C "${ROOT}" rev-parse --abbrev-ref HEAD)}"
REMOTE_NAME="${REMOTE_NAME:-origin}"
REMOTE_URL="${REMOTE_URL:-$(git -C "${ROOT}" remote get-url "${REMOTE_NAME}")}" 
RUNS="${PERF_RUNS:-3}"
SUITES="${PERF_SUITES:-micro meso macro linux}"
SYNC_MODE="${SYNC_MODE:-auto}"  # auto|git|archive
START_INSTANCE="${START_INSTANCE:-1}"
STOP_WHEN_DONE="${STOP_WHEN_DONE:-1}"
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${ROOT}/.artifacts/perf/gcp/${INSTANCE}-${STAMP}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"

cleanup() {
  if [[ "${STOP_WHEN_DONE}" == "1" ]]; then
    gcloud compute instances stop "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null 2>&1 || true
  fi
}
trap cleanup EXIT

if [[ "${START_INSTANCE}" == "1" ]]; then
  gcloud compute instances start "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null || true
fi

prepare_remote='\
set -euo pipefail
sudo apt-get update
sudo DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
if ! command -v cargo >/dev/null 2>&1; then
  curl https://sh.rustup.rs -sSf | sh -s -- -y
fi
'

gcloud compute ssh "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command "${prepare_remote}"

git_sync='\
set -euo pipefail
if [[ -d "$HOME/wrela/.git" ]]; then
  cd "$HOME/wrela"
  git remote set-url origin "__REMOTE_URL__"
  git fetch origin "__BRANCH__"
  git checkout -B "__BRANCH__" "origin/__BRANCH__"
else
  rm -rf "$HOME/wrela"
  git clone --branch "__BRANCH__" --single-branch "__REMOTE_URL__" "$HOME/wrela"
fi
'

git_sync="${git_sync/__REMOTE_URL__/${REMOTE_URL}}"
git_sync="${git_sync/__BRANCH__/${BRANCH}}"
git_sync="${git_sync/__BRANCH__/${BRANCH}}"
git_sync="${git_sync/__BRANCH__/${BRANCH}}"

do_archive_sync() {
  local tarball="/tmp/wrela-branch-sync-${STAMP}.tar.gz"
  (cd "${ROOT}" && COPYFILE_DISABLE=1 tar --exclude .git --exclude target --exclude .artifacts --exclude '.DS_Store' --exclude '._*' -czf "${tarball}" .)
  gcloud compute scp --project "${PROJECT}" --zone "${ZONE}" "${tarball}" "${INSTANCE}:~/wrela-sync.tar.gz" --quiet
  gcloud compute ssh "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command '\
set -euo pipefail
rm -rf "$HOME/wrela"
mkdir -p "$HOME/wrela"
tar -xzf "$HOME/wrela-sync.tar.gz" -C "$HOME/wrela"
find "$HOME/wrela" -name "._*" -type f -delete
'
}

case "${SYNC_MODE}" in
  git)
    gcloud compute ssh "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command "${git_sync}"
    ;;
  archive)
    do_archive_sync
    ;;
  auto)
    if ! gcloud compute ssh "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command "${git_sync}"; then
      echo "Git sync failed on VM, falling back to archive sync"
      do_archive_sync
    fi
    ;;
  *)
    echo "SYNC_MODE must be one of: auto, git, archive" >&2
    exit 1
    ;;
esac

run_perf_cmd="$(cat <<EOF
set -euo pipefail
source "\$HOME/.cargo/env"
cd "\$HOME/wrela"
find . -name "._*" -type f -delete
for suite in ${SUITES}; do
  cargo run -p wrela --release -- perf --runs=${RUNS} --baseline-out=".artifacts/perf/\${suite}-baseline.json" "benchmarks/\${suite}"
done
uname -a > .artifacts/perf/host.txt
EOF
)"

gcloud compute ssh "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet --command "${run_perf_cmd}"

gcloud compute scp --project "${PROJECT}" --zone "${ZONE}" --recurse "${INSTANCE}:~/wrela/.artifacts/perf" "${OUT_DIR}" --quiet

echo "Artifacts: ${OUT_DIR}/perf"
