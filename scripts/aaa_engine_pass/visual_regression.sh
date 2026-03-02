#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ARTIFACT_ROOT="${WRELA_AAA_ARTIFACT_ROOT:-${ROOT}/.artifacts/aaa-engine-pass}"
BASELINE_PATH="${1:-${WRELA_VISREG_BASELINE_IMAGE:-}}"
CANDIDATE_PATH="${2:-${WRELA_VISREG_CANDIDATE_IMAGE:-}}"
REPORT_PATH="${3:-${WRELA_VISREG_REPORT_PATH:-${ARTIFACT_ROOT}/CI-01/visual-regression-report.json}}"
SUMMARY_PATH="${ARTIFACT_ROOT}/CI-01/summary.md"
LANE_ARTIFACT_PATH="${ARTIFACT_ROOT}/CI-01/lane-artifact.json"

fail() {
  echo "visual regression failed: $*" >&2
  exit 1
}

if [[ -z "$BASELINE_PATH" || -z "$CANDIDATE_PATH" ]]; then
  fail "usage: visual_regression.sh <baseline-image> <candidate-image> [report-path]"
fi

if [[ ! -f "$BASELINE_PATH" ]]; then
  fail "baseline image missing: ${BASELINE_PATH}"
fi
if [[ ! -f "$CANDIDATE_PATH" ]]; then
  fail "candidate image missing: ${CANDIDATE_PATH}"
fi

if [[ ! -f "$LANE_ARTIFACT_PATH" ]]; then
  fail "missing lane artifact: ${LANE_ARTIFACT_PATH}"
fi

mkdir -p "$(dirname "$REPORT_PATH")"
mkdir -p "$(dirname "$SUMMARY_PATH")"

sha256_file() {
  local path="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$path" | awk '{print $1}'
    return
  fi
  shasum -a 256 "$path" | awk '{print $1}'
}

base_hash="$(sha256_file "$BASELINE_PATH")"
cand_hash="$(sha256_file "$CANDIDATE_PATH")"

status="pass"
if [[ "$base_hash" != "$cand_hash" ]]; then
  status="fail"
fi

node - "$REPORT_PATH" "$BASELINE_PATH" "$CANDIDATE_PATH" "$base_hash" "$cand_hash" "$status" <<'NODE'
const fs = require('node:fs');
const [path, baselinePath, candidatePath, baselineHash, candidateHash, status] = process.argv.slice(2);
const payload = {
  schema_version: 1,
  kind: 'aaa-visual-regression-report-v1',
  baseline: baselinePath,
  candidate: candidatePath,
  baseline_sha256: baselineHash,
  candidate_sha256: candidateHash,
  status,
};
fs.writeFileSync(path, JSON.stringify(payload, null, 2));
NODE

cat > "$SUMMARY_PATH" <<SUMMARY
# AAA Visual Regression Summary

1. baseline: ${BASELINE_PATH}
2. candidate: ${CANDIDATE_PATH}
3. baseline_sha256: ${base_hash}
4. candidate_sha256: ${cand_hash}
5. status: ${status}
SUMMARY

if [[ "$status" != "pass" ]]; then
  fail "image hashes differ (baseline=${base_hash} candidate=${cand_hash})"
fi

echo "visual regression: PASS"
