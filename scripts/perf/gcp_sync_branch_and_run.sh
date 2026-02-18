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
TARGET_REF="${TARGET_REF:-}"
REMOTE_NAME="${REMOTE_NAME:-origin}"
REMOTE_URL="${REMOTE_URL:-$(git -C "${ROOT}" remote get-url "${REMOTE_NAME}")}" 
RUNS="${PERF_RUNS:-3}"
SUITES="${PERF_SUITES:-micro meso macro linux}"
SYNC_MODE="${SYNC_MODE:-auto}"  # auto|git|archive
INSTALL_DEPS_ON_VM="${INSTALL_DEPS_ON_VM:-auto}" # auto|always|never
FORCE_REBUILD_WRELA="${FORCE_REBUILD_WRELA:-1}"
START_INSTANCE="${START_INSTANCE:-1}"
STOP_WHEN_DONE="${STOP_WHEN_DONE:-1}"
STOP_MODE="${STOP_MODE:-stop}" # stop|delete
STAMP="$(date +%Y%m%d-%H%M%S)"
OUT_DIR="${ROOT}/.artifacts/perf/gcp/${INSTANCE}-${STAMP}"
SSH_MAX_ATTEMPTS="${GCP_SSH_MAX_RETRIES:-6}"
SSH_RETRY_SLEEP_SEC="${GCP_SSH_RETRY_SLEEP_SEC:-10}"
SSH_CONNECT_TIMEOUT_SEC="${GCP_SSH_CONNECT_TIMEOUT_SEC:-30}"
SCP_MAX_ATTEMPTS="${GCP_SCP_MAX_RETRIES:-4}"
SCP_RETRY_SLEEP_SEC="${GCP_SCP_RETRY_SLEEP_SEC:-8}"

if [[ -z "${PROJECT}" ]]; then
  echo "Missing project. Set GCP_PROJECT or run: gcloud config set project <id>" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"

run_ssh_with_retry() {
  local command="$1"
  local max_attempts="${2:-${SSH_MAX_ATTEMPTS}}"
  local sleep_sec="${3:-${SSH_RETRY_SLEEP_SEC}}"
  local attempt=0

  while (( attempt < max_attempts )); do
    attempt=$((attempt + 1))
    if gcloud compute ssh "${INSTANCE}" \
      --project "${PROJECT}" \
      --zone "${ZONE}" \
      --quiet \
      --ssh-flag="-o ConnectTimeout=${SSH_CONNECT_TIMEOUT_SEC}" \
      --ssh-flag="-o ServerAliveInterval=15" \
      --ssh-flag="-o ServerAliveCountMax=3" \
      --command "${command}"; then
      return 0
    fi
    if (( attempt < max_attempts )); then
      echo "[${INSTANCE}] ssh attempt ${attempt}/${max_attempts} failed; retrying in ${sleep_sec}s" >&2
      sleep "${sleep_sec}"
    fi
  done

  return 1
}

run_scp_with_retry() {
  local src="$1"
  local dst="$2"
  local recurse="${3:-0}"
  local max_attempts="${4:-${SCP_MAX_ATTEMPTS}}"
  local sleep_sec="${5:-${SCP_RETRY_SLEEP_SEC}}"
  local attempt=0

  while (( attempt < max_attempts )); do
    attempt=$((attempt + 1))
    if [[ "${recurse}" == "1" ]]; then
      if gcloud compute scp \
        --project "${PROJECT}" \
        --zone "${ZONE}" \
        --recurse \
        --quiet \
        --scp-flag="-o ConnectTimeout=${SSH_CONNECT_TIMEOUT_SEC}" \
        "${src}" "${dst}"; then
        return 0
      fi
    else
      if gcloud compute scp \
        --project "${PROJECT}" \
        --zone "${ZONE}" \
        --quiet \
        --scp-flag="-o ConnectTimeout=${SSH_CONNECT_TIMEOUT_SEC}" \
        "${src}" "${dst}"; then
        return 0
      fi
    fi

    if (( attempt < max_attempts )); then
      echo "[${INSTANCE}] scp attempt ${attempt}/${max_attempts} failed; retrying in ${sleep_sec}s" >&2
      sleep "${sleep_sec}"
    fi
  done

  return 1
}

cleanup() {
  if [[ "${STOP_WHEN_DONE}" == "1" ]]; then
    if [[ "${STOP_MODE}" == "delete" ]]; then
      gcloud compute instances delete "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" --quiet >/dev/null 2>&1 || true
    else
      gcloud compute instances stop "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null 2>&1 || true
    fi
  fi
}
trap cleanup EXIT

if [[ "${START_INSTANCE}" == "1" ]]; then
  gcloud compute instances start "${INSTANCE}" --project "${PROJECT}" --zone "${ZONE}" >/dev/null || true
fi

prepare_remote='\
set -euo pipefail
mode="__INSTALL_DEPS_MODE__"
need_install=0
if [[ -f "$HOME/.cargo/env" ]]; then
  source "$HOME/.cargo/env"
fi
if [[ "${mode}" == "always" ]]; then
  need_install=1
elif [[ "${mode}" == "auto" ]]; then
  for c in git jq clang; do
    if ! command -v "$c" >/dev/null 2>&1; then
      need_install=1
      break
    fi
  done
  if [[ "${need_install}" == "0" ]]; then
    if ! command -v cargo >/dev/null 2>&1 && [[ ! -x "$HOME/.cargo/bin/cargo" ]]; then
      need_install=1
    fi
  fi
fi
if [[ "${need_install}" == "1" ]]; then
  sudo apt-get update
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
  if ! command -v cargo >/dev/null 2>&1; then
    curl https://sh.rustup.rs -sSf | sh -s -- -y
  fi
fi
'
prepare_remote="${prepare_remote//__INSTALL_DEPS_MODE__/${INSTALL_DEPS_ON_VM}}"

run_ssh_with_retry "${prepare_remote}"

git_sync='\
set -euo pipefail
target_ref="__TARGET_REF__"
branch_name="__BRANCH__"
remote_url="__REMOTE_URL__"
if [[ -d "$HOME/wrela/.git" ]]; then
  cd "$HOME/wrela"
  git remote set-url origin "${remote_url}"
else
  rm -rf "$HOME/wrela"
  git clone "${remote_url}" "$HOME/wrela"
  cd "$HOME/wrela"
fi
if [[ -n "${target_ref}" ]]; then
  git fetch origin "${target_ref}"
  git checkout --detach FETCH_HEAD
else
  git fetch origin "${branch_name}"
  git checkout -B "${branch_name}" "origin/${branch_name}"
fi
'

git_sync="${git_sync//__REMOTE_URL__/${REMOTE_URL}}"
git_sync="${git_sync//__BRANCH__/${BRANCH}}"
git_sync="${git_sync//__TARGET_REF__/${TARGET_REF}}"

do_archive_sync() {
  local tarball="/tmp/wrela-branch-sync-${STAMP}.tar.gz"
  (cd "${ROOT}" && COPYFILE_DISABLE=1 tar --exclude .git --exclude target --exclude .artifacts --exclude '.DS_Store' --exclude '._*' -czf "${tarball}" .)
  run_scp_with_retry "${tarball}" "${INSTANCE}:~/wrela-sync.tar.gz"
  run_ssh_with_retry '\
set -euo pipefail
rm -rf "$HOME/wrela"
mkdir -p "$HOME/wrela"
tar -xzf "$HOME/wrela-sync.tar.gz" -C "$HOME/wrela"
find "$HOME/wrela" -name "._*" -type f -delete
'
}

case "${SYNC_MODE}" in
  git)
    run_ssh_with_retry "${git_sync}"
    ;;
  archive)
    do_archive_sync
    ;;
  auto)
    if ! run_ssh_with_retry "${git_sync}"; then
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
if [[ -f "\$HOME/.cargo/env" ]]; then
  source "\$HOME/.cargo/env"
fi
cd "\$HOME/wrela"
find . -name "._*" -type f -delete
if [[ "${FORCE_REBUILD_WRELA}" == "1" ]]; then
  cargo build -p wrela --release
fi
for suite in ${SUITES}; do
  ./target/release/wrela perf --runs=${RUNS} --baseline-out=".artifacts/perf/\${suite}-baseline.json" "benchmarks/\${suite}"
done
uname -a > .artifacts/perf/host.txt
EOF
)"

run_ssh_with_retry "${run_perf_cmd}" 1

copy_artifacts() {
  if run_scp_with_retry "${INSTANCE}:~/wrela/.artifacts/perf" "${OUT_DIR}" 1; then
    return 0
  fi

  # Fallback path for intermittent scp transport/parser failures.
  if run_ssh_with_retry 'set -euo pipefail; tar -C "$HOME/wrela/.artifacts" -czf - perf' 2 5 > "${OUT_DIR}/perf.tar.gz"; then
    mkdir -p "${OUT_DIR}/perf"
    tar -xzf "${OUT_DIR}/perf.tar.gz" -C "${OUT_DIR}"
    rm -f "${OUT_DIR}/perf.tar.gz"
    return 0
  fi
  return 1
}

if ! copy_artifacts; then
  echo "error: failed to copy perf artifacts from ${INSTANCE}" >&2
  exit 1
fi

echo "Artifacts: ${OUT_DIR}/perf"
