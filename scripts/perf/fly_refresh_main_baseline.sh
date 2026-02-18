#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<USAGE >&2
Usage: $0 [--sha <main-sha>] [--run-id <id>]
USAGE
}

TARGET_SHA=""
RUN_ID=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha) TARGET_SHA="${2:-}"; shift 2 ;;
    --run-id) RUN_ID="${2:-}"; shift 2 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown arg: $1" >&2; usage; exit 1 ;;
  esac
done

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CANONICAL_DIR="${ROOT}/.artifacts/perf/main"
CANONICAL_PATH="${CANONICAL_DIR}/CANONICAL.json"
mkdir -p "${CANONICAL_DIR}"

resolve_main_head() {
  local head
  if head="$(git -C "${ROOT}" ls-remote origin refs/heads/main 2>/dev/null | awk '{print $1}' | head -n1)" && [[ -n "${head}" ]]; then
    echo "${head}"
    return 0
  fi
  git -C "${ROOT}" rev-parse origin/main
}

if [[ -z "${TARGET_SHA}" ]]; then
  TARGET_SHA="$(resolve_main_head)"
fi

short_sha="$(echo "${TARGET_SHA}" | cut -c1-12)"
if [[ -z "${RUN_ID}" ]]; then
  RUN_ID="main-refresh-$(date +%Y%m%d-%H%M%S)-${short_sha}"
fi

run_log="${ROOT}/.artifacts/perf/fly/${RUN_ID}-refresh.log"
mkdir -p "$(dirname "${run_log}")"

set +e
"${ROOT}/scripts/perf/fly_pr_perf_gate.sh" --sha "${TARGET_SHA}" --run-id "${RUN_ID}" >"${run_log}" 2>&1
gate_rc=$?
set -e

summary_path="$(sed -n 's/^Perf summary: //p' "${run_log}" | tail -n1)"
if [[ -z "${summary_path}" || ! -f "${summary_path}" ]]; then
  echo "failed: missing perf summary from fly_pr_perf_gate" >&2
  cat "${run_log}" >&2
  exit 1
fi

overall_status="$(jq -r '.overall.status // ""' "${summary_path}")"
overall_reason="$(jq -r '.overall.reason // ""' "${summary_path}")"
run_id_from_summary="$(jq -r '.run_id // ""' "${summary_path}")"
if [[ -z "${run_id_from_summary}" ]]; then
  run_id_from_summary="${RUN_ID}"
fi

refresh_state="failed"
update_pointer=0

if [[ "${overall_status}" == "passed" && ${gate_rc} -eq 0 ]]; then
  current_main_head="$(resolve_main_head)"
  if [[ "${TARGET_SHA}" != "${current_main_head}" ]]; then
    refresh_state="stale"
  else
    refresh_state="passed"
    update_pointer=1
  fi
else
  case "${overall_reason}" in
    perf_failed) refresh_state="perf_failed" ;;
    infra_unavailable) refresh_state="infra_unavailable" ;;
    *) refresh_state="infra_error" ;;
  esac
fi

baseline_dir="${CANONICAL_DIR}/${TARGET_SHA}"
if [[ ${update_pointer} -eq 1 ]]; then
  rm -rf "${baseline_dir}"
  mkdir -p "${baseline_dir}"
  cp -R "$(dirname "${summary_path}")/." "${baseline_dir}/"

  generated_at_unix_ms="$(( $(date +%s) * 1000 ))"
  tmp_pointer="${CANONICAL_PATH}.tmp"
  cat > "${tmp_pointer}" <<JSON
{
  "version": 2,
  "sha": "${TARGET_SHA}",
  "status": "passed",
  "generated_at_unix_ms": ${generated_at_unix_ms},
  "run_id": "${run_id_from_summary}",
  "arch_scope": "amd64_only",
  "artifacts_root": "${baseline_dir}",
  "summary_path": "${baseline_dir}/summary.json",
  "suites": "${PERF_SUITES:-micro meso macro linux}",
  "profile": "${PERF_PROFILE:-standard}"
}
JSON
  mv "${tmp_pointer}" "${CANONICAL_PATH}"
fi

refresh_report="${CANONICAL_DIR}/refresh-${run_id_from_summary}.json"
generated_at_unix_ms="$(( $(date +%s) * 1000 ))"
cat > "${refresh_report}" <<JSON
{
  "version": 1,
  "run_id": "${run_id_from_summary}",
  "target_sha": "${TARGET_SHA}",
  "state": "${refresh_state}",
  "gate_rc": ${gate_rc},
  "overall_reason": "${overall_reason}",
  "summary_path": "${summary_path}",
  "canonical_pointer": "${CANONICAL_PATH}",
  "generated_at_unix_ms": ${generated_at_unix_ms}
}
JSON

echo "Main refresh report: ${refresh_report}"
if [[ ${update_pointer} -eq 1 ]]; then
  echo "Canonical pointer updated: ${CANONICAL_PATH} -> ${TARGET_SHA}"
fi

case "${refresh_state}" in
  passed|stale) exit 0 ;;
  infra_unavailable) exit 2 ;;
  perf_failed) exit 1 ;;
  *) exit 1 ;;
esac
