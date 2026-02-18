#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 --app <name> --machine <id> --sha <commit> [--out-dir <path>]

Environment:
  PERF_SUITES           (default: micro meso macro linux)
  PERF_RUNS             (default: 10)
  PERF_CV_MAX_PCT       (default: 10)
  PERF_WARMUP_RUNS      (default: 1)
  FORCE_REBUILD_WRELA   (default: 1)
  INSTALL_DEPS_ON_VM    (default: auto) # auto|always|never
  REMOTE_REF            (default: refs/heads/main)
USAGE
}

APP=""
MACHINE_ID=""
SHA=""
OUT_DIR=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app) APP="${2:-}"; shift 2 ;;
    --machine) MACHINE_ID="${2:-}"; shift 2 ;;
    --sha) SHA="${2:-}"; shift 2 ;;
    --out-dir) OUT_DIR="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${APP}" || -z "${MACHINE_ID}" || -z "${SHA}" ]]; then
  usage
  exit 1
fi

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SUITES="${PERF_SUITES:-micro meso macro linux}"
RUNS="${PERF_RUNS:-10}"
PERF_CV_MAX_PCT="${PERF_CV_MAX_PCT:-10}"
PERF_WARMUP_RUNS="${PERF_WARMUP_RUNS:-1}"
FORCE_REBUILD_WRELA="${FORCE_REBUILD_WRELA:-1}"
INSTALL_DEPS_ON_VM="${INSTALL_DEPS_ON_VM:-auto}"
REMOTE_URL="${REMOTE_URL:-$(git -C "${ROOT}" remote get-url origin)}"
REMOTE_REF="${REMOTE_REF:-refs/heads/main}"

if [[ -z "${OUT_DIR}" ]]; then
  stamp="$(date +%Y%m%d-%H%M%S)"
  OUT_DIR="${ROOT}/.artifacts/perf/fly/${APP}-${MACHINE_ID}-${stamp}"
fi
mkdir -p "${OUT_DIR}"

remote_cmd="$(cat <<'RCMD'
set -euo pipefail
if [[ -f "$HOME/.cargo/env" ]]; then
  source "$HOME/.cargo/env"
fi

mode="__INSTALL_DEPS_ON_VM__"
need_install=0
if [[ "${mode}" == "always" ]]; then
  need_install=1
elif [[ "${mode}" == "auto" ]]; then
  for c in git jq clang; do
    if ! command -v "$c" >/dev/null 2>&1; then
      need_install=1
      break
    fi
  done
  if [[ "${need_install}" == "0" ]] && ! command -v cargo >/dev/null 2>&1; then
    need_install=1
  fi
fi

if [[ "${need_install}" == "1" ]]; then
  apt_prefix=""
  if command -v sudo >/dev/null 2>&1; then
    apt_prefix="sudo"
  fi
  ${apt_prefix} apt-get update
  ${apt_prefix} DEBIAN_FRONTEND=noninteractive apt-get install -y build-essential pkg-config libssl-dev clang llvm make git curl ca-certificates jq
  if ! command -v cargo >/dev/null 2>&1; then
    curl https://sh.rustup.rs -sSf | sh -s -- -y
    source "$HOME/.cargo/env"
  fi
fi

if [[ -d "$HOME/wrela/.git" ]]; then
  cd "$HOME/wrela"
  git remote set-url origin __REMOTE_URL__ || true
else
  rm -rf "$HOME/wrela"
  git clone __REMOTE_URL__ "$HOME/wrela"
  cd "$HOME/wrela"
fi

git fetch origin __REMOTE_REF__
if ! git cat-file -e "__SHA__^{commit}" >/dev/null 2>&1; then
  echo "error: target sha __SHA__ not found after fetching __REMOTE_REF__" >&2
  exit 1
fi
git checkout --detach __SHA__

pkill -f "target/release/wrela perf" >/dev/null 2>&1 || true
rm -rf .artifacts/perf
mkdir -p .artifacts/perf

# Seed from baked bootstrap build to reduce first-run rebuild cost.
if [[ ! -d "target/release" && -d "/opt/wrela-bootstrap/target/release" ]]; then
  mkdir -p target
  cp -R /opt/wrela-bootstrap/target/release target/ || true
fi

if [[ "__FORCE_REBUILD_WRELA__" == "1" ]]; then
  cargo build -p wrela --release
fi

cv_arg=""
if [[ -n "__PERF_CV_MAX_PCT__" ]]; then
  cv_arg="--perf-cv-max-pct=__PERF_CV_MAX_PCT__"
fi

for suite in __SUITES__; do
  if [[ "__PERF_WARMUP_RUNS__" -gt 0 ]]; then
    for _warm in $(seq 1 "__PERF_WARMUP_RUNS__"); do
      # Warmup run: intentionally ignored and not used as baseline.
      ./target/release/wrela perf --runs=1 --perf-cv-max-pct=100 --baseline-out=".artifacts/perf/${suite}-warmup.json" "benchmarks/${suite}" >/dev/null
    done
  fi
  ./target/release/wrela perf --runs=__RUNS__ ${cv_arg} --baseline-out=".artifacts/perf/${suite}-baseline.json" "benchmarks/${suite}"
done

uname -a > .artifacts/perf/host.txt
RCMD
)"

remote_cmd="${remote_cmd//__INSTALL_DEPS_ON_VM__/${INSTALL_DEPS_ON_VM}}"
remote_cmd="${remote_cmd//__SHA__/${SHA}}"
remote_cmd="${remote_cmd//__FORCE_REBUILD_WRELA__/${FORCE_REBUILD_WRELA}}"
remote_cmd="${remote_cmd//__SUITES__/${SUITES}}"
remote_cmd="${remote_cmd//__RUNS__/${RUNS}}"
remote_cmd="${remote_cmd//__PERF_WARMUP_RUNS__/${PERF_WARMUP_RUNS}}"
remote_cmd="${remote_cmd//__REMOTE_URL__/${REMOTE_URL}}"
remote_cmd="${remote_cmd//__REMOTE_REF__/${REMOTE_REF}}"
remote_cmd="${remote_cmd//__PERF_CV_MAX_PCT__/${PERF_CV_MAX_PCT}}"

remote_script_b64="$(printf "%s\n" "${remote_cmd}" | base64 | tr -d '\n')"
remote_exec_cmd="bash -lc 'set -euo pipefail; echo ${remote_script_b64} | base64 -d >/tmp/wrela-perf-run.sh; chmod 700 /tmp/wrela-perf-run.sh; bash /tmp/wrela-perf-run.sh'"
flyctl ssh console -a "${APP}" --machine "${MACHINE_ID}" --command "${remote_exec_cmd}" >"${OUT_DIR}/run.log" 2>&1

if ! flyctl ssh console -a "${APP}" --machine "${MACHINE_ID}" --command 'bash -lc '\''set -euo pipefail; tar -C "$HOME/wrela/.artifacts" -czf - perf'\''' > "${OUT_DIR}/perf.tar.gz" 2>>"${OUT_DIR}/run.log"; then
  echo "error: failed to copy perf artifacts from ${APP}/${MACHINE_ID}" >&2
  exit 1
fi

tar -xzf "${OUT_DIR}/perf.tar.gz" -C "${OUT_DIR}"
rm -f "${OUT_DIR}/perf.tar.gz"

echo "Artifacts: ${OUT_DIR}/perf"
